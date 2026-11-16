//! Fetching publications: look up the file with the pub-media API, download
//! it with verification, then import it like any user-supplied file.

use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Deserialize;

use crate::catalog::CatalogItem;
use crate::library::{Entry, Library, MediaEntry};
use crate::links::{MediaKind, MediaRef};
use crate::mediator::Media;
use crate::net::{Client, Expected, encode_query};
use crate::{Error, Result};

pub const PUB_MEDIA_URL: &str = "https://b.jw-cdn.org/apis/pub-media/GETPUBMEDIALINKS";

#[derive(Debug, Deserialize)]
struct PubMediaResponse {
    #[serde(default)]
    files: HashMap<String, HashMap<String, Vec<PubMediaFile>>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PubMediaFile {
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub filesize: Option<u64>,
    pub file: PubMediaFileRef,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PubMediaFileRef {
    pub url: String,
    /// MD5 of the file.
    #[serde(default)]
    pub checksum: Option<String>,
}

/// What to download.
#[derive(Debug, Clone)]
pub struct Request<'a> {
    /// Key symbol (`wp`, `nwtsty`) as used by pub-media.
    pub key_symbol: &'a str,
    /// Language code, e.g. `X`.
    pub lang_code: &'a str,
    /// `IssueTagNumber` (e.g. 20260900), 0 or `None` for undated works.
    pub issue_tag: Option<i64>,
}

fn valid_token(s: &str) -> bool {
    !s.is_empty() && s.len() <= 32 && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
}

/// pub-media URL for a JWPUB file.
pub fn pub_media_url(req: &Request<'_>) -> Result<String> {
    if !valid_token(req.key_symbol) || !valid_token(req.lang_code) {
        return Err(Error::NotFound(format!(
            "invalid publication or language: {:?} {:?}",
            req.key_symbol, req.lang_code
        )));
    }
    let mut url = format!(
        "{PUB_MEDIA_URL}?output=json&pub={}&fileformat=JWPUB&alllangs=0&langwritten={}",
        encode_query(req.key_symbol),
        encode_query(req.lang_code)
    );
    if let Some(tag) = req.issue_tag.filter(|t| *t > 0) {
        // 20260900 -> 202609; semi-monthly issues keep the day: 20071215
        let issue = if tag % 100 == 0 { tag / 100 } else { tag };
        url.push_str(&format!("&issue={issue}"));
    }
    Ok(url)
}

/// Look up the JWPUB file(s) for a publication.
pub fn jwpub_links(client: &Client, req: &Request<'_>) -> Result<Vec<PubMediaFile>> {
    let url = pub_media_url(req)?;
    let resp: PubMediaResponse = client.get_json(&url).map_err(|e| match e {
        Error::Http(msg) if msg.contains("HTTP 400") || msg.contains("HTTP 404") => {
            Error::NotFound(format!(
                "{} ({}) is not available as JWPUB",
                req.key_symbol, req.lang_code
            ))
        }
        e => e,
    })?;
    Ok(resp
        .files
        .into_iter()
        .filter(|(lang, _)| lang.eq_ignore_ascii_case(req.lang_code))
        .flat_map(|(_, formats)| formats.into_iter())
        .filter(|(fmt, _)| fmt.eq_ignore_ascii_case("JWPUB"))
        .flat_map(|(_, files)| files)
        .collect())
}

#[derive(Debug, Deserialize)]
struct MediaResponse {
    #[serde(default)]
    files: HashMap<String, HashMap<String, Vec<RawMediaFile>>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawMediaFile {
    #[serde(default)]
    title: String,
    #[serde(default)]
    label: String,
    #[serde(default)]
    mimetype: String,
    #[serde(default)]
    filesize: Option<u64>,
    #[serde(default)]
    duration: Option<f64>,
    #[serde(default)]
    frame_height: Option<u32>,
    file: PubMediaFileRef,
    #[serde(default)]
    track_image: Option<ImageRef>,
    #[serde(default)]
    subtitles: Option<ImageRef>,
}

#[derive(Debug, Deserialize)]
struct ImageRef {
    #[serde(default)]
    url: String,
}

/// One playable rendition of a recording.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct MediaFile {
    pub title: String,
    /// Quality such as `360p`; empty for audio.
    pub label: String,
    pub mime: String,
    pub url: String,
    pub size: Option<u64>,
    /// Length in seconds.
    pub duration: Option<f64>,
    /// Still image to show before playback.
    pub poster: Option<String>,
    /// WebVTT captions, when the recording has them.
    pub subtitles: Option<String>,
    /// MD5 of the file.
    pub md5: Option<String>,
}

/// pub-media URL for the MP4 or MP3 files of one track.
pub fn media_url(media: &MediaRef) -> Result<String> {
    if !valid_token(&media.pub_symbol) || !valid_token(&media.lang_code) {
        return Err(Error::NotFound(format!(
            "invalid recording: {:?} {:?}",
            media.pub_symbol, media.lang_code
        )));
    }
    let format = match media.kind {
        MediaKind::Video => "MP4",
        MediaKind::Audio => "MP3",
    };
    let mut url = format!(
        "{PUB_MEDIA_URL}?output=json&pub={}&fileformat={format}&alllangs=0&langwritten={}&track={}",
        encode_query(&media.pub_symbol),
        encode_query(&media.lang_code),
        media.track
    );
    if let Some(issue) = media.issue.as_deref() {
        url.push_str(&format!("&issue={}", encode_query(issue)));
    }
    Ok(url)
}

/// Look up the renditions of a recording, smallest first. Only files on the
/// allowed hosts are returned; the player streams them, nothing is stored.
pub fn media_links(client: &Client, media: &MediaRef) -> Result<Vec<MediaFile>> {
    let resp: MediaResponse = client.get_json(&media_url(media)?).map_err(|e| match e {
        Error::Http(msg) if msg.contains("HTTP 400") || msg.contains("HTTP 404") => {
            Error::NotFound(format!(
                "{} track {} is not available in {}",
                media.pub_symbol, media.track, media.lang_code
            ))
        }
        e => e,
    })?;
    let prefix = match media.kind {
        MediaKind::Video => "video/",
        MediaKind::Audio => "audio/",
    };
    let mut files: Vec<(u32, MediaFile)> = resp
        .files
        .into_iter()
        .filter(|(lang, _)| lang.eq_ignore_ascii_case(&media.lang_code))
        .flat_map(|(_, formats)| formats.into_values().flatten())
        .filter(|f| f.mimetype.starts_with(prefix) && client.check_url(&f.file.url).is_ok())
        .map(|f| {
            let poster = f
                .track_image
                .map(|i| i.url)
                .filter(|u| client.check_url(u).is_ok());
            let subtitles = f
                .subtitles
                .map(|s| s.url)
                .filter(|u| !u.is_empty() && client.check_url(u).is_ok());
            (
                f.frame_height.unwrap_or(0),
                MediaFile {
                    title: f.title,
                    label: f.label,
                    mime: f.mimetype,
                    url: f.file.url,
                    size: f.filesize,
                    duration: f.duration,
                    poster,
                    subtitles,
                    md5: f.file.checksum.filter(|c| !c.is_empty()),
                },
            )
        })
        .collect();
    files.sort_by_key(|(height, f)| (*height, f.size));
    Ok(files.into_iter().map(|(_, f)| f).collect())
}

/// Download a publication and import it. `catalog_item`, when known, adds a
/// SHA-1 and size check and is used to confirm the imported publication.
pub fn download(
    client: &Client,
    library: &mut Library,
    download_dir: &Path,
    req: &Request<'_>,
    catalog_item: Option<&CatalogItem>,
    progress: &mut dyn FnMut(u64, Option<u64>),
) -> Result<Entry> {
    let file = jwpub_links(client, req)?
        .into_iter()
        .next()
        .ok_or_else(|| Error::NotFound(format!("no JWPUB file for {}", req.key_symbol)))?;
    client.check_url(&file.file.url)?;

    let expected = Expected {
        size: catalog_item.map(|c| c.size).or(file.filesize),
        md5: file.file.checksum.clone().filter(|c| !c.is_empty()),
        sha1: catalog_item.map(|c| c.sha1.clone()),
    };
    if let (Some(c), Some(f)) = (catalog_item, file.filesize)
        && c.size != f
    {
        return Err(Error::HashMismatch {
            what: "catalog vs pub-media size",
            expected: c.size.to_string(),
            actual: f.to_string(),
        });
    }

    let name = format!(
        "{}_{}_{}.jwpub",
        req.key_symbol,
        req.lang_code,
        req.issue_tag.unwrap_or(0)
    );
    let dest = download_dir.join(name);
    client.download(&file.file.url, &dest, &expected, progress)?;
    let result = import_download(library, &dest, catalog_item);
    let _ = fs::remove_file(&dest);
    result
}

fn import_download(
    library: &mut Library,
    dest: &Path,
    catalog_item: Option<&CatalogItem>,
) -> Result<Entry> {
    library.import_checked(dest, |entry| {
        if let Some(c) = catalog_item {
            let issue_ok = entry.issue_tag.parse::<i64>().unwrap_or(0) == c.issue_tag;
            if entry.meps_language != c.meps_language || entry.symbol != c.symbol || !issue_ok {
                return Err(Error::NotFound(format!(
                    "downloaded {} ({} {}) but expected {} ({} {})",
                    entry.symbol,
                    entry.meps_language,
                    entry.issue_tag,
                    c.symbol,
                    c.meps_language,
                    c.issue_tag
                )));
            }
        }
        Ok(())
    })
}

/// File extension for a recording's MIME type, if it is one we save.
fn media_extension(mime: &str) -> Option<&'static str> {
    match mime {
        "video/mp4" => Some("mp4"),
        "audio/mpeg" => Some("mp3"),
        "audio/mp4" | "audio/x-m4a" => Some("m4a"),
        _ => None,
    }
}

/// Download one rendition of a recording (and its captions, if any) into the
/// library. The video or audio file is verified against the size and MD5 from
/// the catalog. Downloading another quality of the same recording replaces it.
pub fn download_media(
    client: &Client,
    library: &Library,
    lang_code: &str,
    media: &Media,
    label: &str,
    progress: &mut dyn FnMut(u64, Option<u64>),
) -> Result<MediaEntry> {
    let file = media
        .files
        .iter()
        .find(|f| f.label == label)
        .ok_or_else(|| Error::NotFound(format!("{} in {label}", media.title)))?;
    client.check_url(&file.url)?;
    let ext = media_extension(&file.mime)
        .ok_or_else(|| Error::NotFound(format!("cannot save {}", file.mime)))?;

    let stem = library.media_file_stem(&media.key, lang_code, label)?;
    let name = format!("{stem}.{ext}");
    let dest = library.media_dir().join(&name);
    let expected = Expected {
        size: file.size,
        md5: file.md5.clone(),
        sha1: None,
    };
    client.download(&file.url, &dest, &expected, progress)?;

    // Captions are optional: a failure leaves the recording without them.
    let subtitle_file = file.subtitles.as_deref().and_then(|url| {
        let name = format!("{stem}.vtt");
        let path = library.media_dir().join(&name);
        client.check_url(url).ok()?;
        let mut ignore = |_: u64, _: Option<u64>| {};
        let saved = client
            .download(url, &path, &Expected::default(), &mut ignore)
            .is_ok();
        if saved {
            return Some(name);
        }
        let _ = fs::remove_file(crate::net::part_path(&path));
        earlier_captions(library, &media.key, lang_code, &name)
    });

    library.add_media(&MediaEntry {
        id: 0,
        key: media.key.clone(),
        lang_code: lang_code.to_owned(),
        title: media.title.clone(),
        kind: media.kind,
        label: label.to_owned(),
        file_name: name,
        subtitle_file,
        size: i64::try_from(fs::metadata(&dest)?.len()).unwrap_or(i64::MAX),
        duration: file.duration.or(media.duration),
        image: media.image.clone(),
        downloaded_at: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(0)),
    })
}

/// Captions to record under `name` (in [`Library::media_dir`]) after their
/// download failed: those an earlier download of this recording left there
/// (same quality), or else a copy of the earlier download's captions, which
/// are the same in every quality. `None` if there were none.
fn earlier_captions(library: &Library, key: &str, lang_code: &str, name: &str) -> Option<String> {
    let path = library.media_dir().join(name);
    if path.is_file() {
        return Some(name.to_owned());
    }
    let old = library.find_media(key, lang_code).ok()??.subtitle_file?;
    if fs::copy(library.media_dir().join(old), &path).is_err() {
        let _ = fs::remove_file(&path);
        return None;
    }
    Some(name.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_mismatch_does_not_replace_existing_publication() {
        let tmp = tempfile::tempdir().unwrap();
        let mut lib = Library::open(tmp.path()).unwrap();
        let path = tmp.path().join("test.jwpub");
        let db = tmp.path().join("source.db");
        let conn = rusqlite::Connection::open(&db).unwrap();
        conn.execute_batch("CREATE TABLE Publication (PublicationId INTEGER PRIMARY KEY, Title TEXT, ShortTitle TEXT, Symbol TEXT, Year INTEGER, MepsLanguageIndex INTEGER, IssueTagNumber TEXT, PublicationType TEXT); INSERT INTO Publication VALUES (1, 'Test', 'Test', 'tst', 2024, 0, '0', 'Book'); CREATE TABLE Document (DocumentId INTEGER PRIMARY KEY);").unwrap();
        drop(conn);
        let db_bytes = fs::read(&db).unwrap();
        use sha1::Digest;
        use std::io::{Cursor, Write};
        let options = zip::write::SimpleFileOptions::default();
        let mut inner = zip::ZipWriter::new(Cursor::new(Vec::new()));
        inner.start_file("tst_E.db", options).unwrap();
        inner.write_all(&db_bytes).unwrap();
        let contents = inner.finish().unwrap().into_inner();
        let manifest = serde_json::json!({"name":"tst_E.jwpub", "hash":hex::encode(sha2::Sha256::digest(&contents)), "timestamp":"2024-01-01T00:00:00Z", "version":1, "expandedSize":db_bytes.len(), "contentFormat":"z-a", "mepsPlatformVersion":"2.10", "publication":{"fileName":"tst_E.db", "title":"Test", "symbol":"tst", "language":0, "year":2024, "hash":hex::encode(sha1::Sha1::digest(&db_bytes))}});
        let mut outer = zip::ZipWriter::new(std::fs::File::create(&path).unwrap());
        outer.start_file("manifest.json", options).unwrap();
        outer
            .write_all(serde_json::to_string(&manifest).unwrap().as_bytes())
            .unwrap();
        outer.start_file("contents", options).unwrap();
        outer.write_all(&contents).unwrap();
        outer.finish().unwrap();
        let original = lib.import(&path).unwrap();
        let marker = lib.publication_dir(&original).join("keep.txt");
        fs::write(&marker, "keep").unwrap();
        let catalog = CatalogItem {
            key_symbol: "wrong".into(),
            symbol: "wrong".into(),
            meps_language: original.meps_language,
            issue_tag: original.issue_tag.parse().unwrap(),
            year: original.year,
            title: "Wrong".into(),
            issue_title: None,
            size: 0,
            sha1: String::new(),
            publication_type: 0,
            short_title: None,
            cataloged_on: None,
            image: None,
            attributes: vec![],
            expanded_size: 0,
        };
        assert!(import_download(&mut lib, &path, Some(&catalog)).is_err());
        assert_eq!(lib.list().unwrap(), vec![original]);
        assert_eq!(fs::read_to_string(marker).unwrap(), "keep");
    }

    #[test]
    fn urls() {
        let u = pub_media_url(&Request {
            key_symbol: "wp",
            lang_code: "X",
            issue_tag: Some(20260900),
        })
        .unwrap();
        assert_eq!(
            u,
            "https://b.jw-cdn.org/apis/pub-media/GETPUBMEDIALINKS?output=json&pub=wp&fileformat=JWPUB&alllangs=0&langwritten=X&issue=202609"
        );
        let u = pub_media_url(&Request {
            key_symbol: "nwtsty",
            lang_code: "X",
            issue_tag: Some(0),
        })
        .unwrap();
        assert!(!u.contains("issue="));
        // Semi-monthly issues need the day, the API has no 200712.
        for (tag, issue) in [(20071215, "20071215"), (20071201, "20071201")] {
            let u = pub_media_url(&Request {
                key_symbol: "w",
                lang_code: "X",
                issue_tag: Some(tag),
            })
            .unwrap();
            assert!(u.ends_with(&format!("&issue={issue}")), "{u}");
        }
        for (pub_, lang) in [("wp&x=1", "X"), ("wp", "X Y"), ("", "X"), ("wp", "../")] {
            assert!(
                pub_media_url(&Request {
                    key_symbol: pub_,
                    lang_code: lang,
                    issue_tag: None
                })
                .is_err()
            );
        }
    }

    #[test]
    fn parses_pub_media_response() {
        let json = r#"{"pubName":"x","files":{"X":{"JWPUB":[{"title":"Regulär","filesize":2861525,
            "file":{"url":"https://cfp2.jw-cdn.org/a/b/wp_X_202609.jwpub","checksum":"3f4a","modifiedDatetime":"x"}}],
            "PDF":[{"title":"p","file":{"url":"https://x/y.pdf"}}]}}}"#;
        let r: PubMediaResponse = serde_json::from_str(json).unwrap();
        let f = &r.files["X"]["JWPUB"][0];
        assert_eq!(f.filesize, Some(2861525));
        assert_eq!(f.file.checksum.as_deref(), Some("3f4a"));
    }

    fn media_ref(issue: Option<&str>, kind: MediaKind) -> MediaRef {
        MediaRef {
            pub_symbol: "mwbv".into(),
            issue: issue.map(str::to_owned),
            track: 3,
            kind,
            lang_code: "X".into(),
        }
    }

    #[test]
    fn media_urls() {
        assert_eq!(
            media_url(&media_ref(Some("202609"), MediaKind::Video)).unwrap(),
            "https://b.jw-cdn.org/apis/pub-media/GETPUBMEDIALINKS?output=json&pub=mwbv&fileformat=MP4&alllangs=0&langwritten=X&track=3&issue=202609"
        );
        assert!(
            media_url(&media_ref(None, MediaKind::Audio))
                .unwrap()
                .contains("fileformat=MP3")
        );
        let mut bad = media_ref(None, MediaKind::Video);
        bad.pub_symbol = "a&b".into();
        assert!(media_url(&bad).is_err());
    }

    #[test]
    fn parses_media_response() {
        let json = r#"{"files":{"X":{"MP4":[{"title":"T","label":"240p","mimetype":"video/mp4",
            "filesize":10,"duration":409.7,"frameHeight":234,
            "file":{"url":"https://cfp2.jw-cdn.org/a/x_r240P.mp4","checksum":"c"},
            "trackImage":{"url":"https://cfp2.jw-cdn.org/a/x.jpg"},"subtitles":{"url":"u"}}]}}}"#;
        let r: MediaResponse = serde_json::from_str(json).unwrap();
        let f = &r.files["X"]["MP4"][0];
        assert_eq!(f.label, "240p");
        assert_eq!(f.frame_height, Some(234));
        assert_eq!(f.duration, Some(409.7));
        assert_eq!(
            f.track_image.as_ref().map(|i| i.url.as_str()),
            Some("https://cfp2.jw-cdn.org/a/x.jpg")
        );
    }

    fn media_entry(label: &str, subtitles: bool) -> MediaEntry {
        let stem = crate::library::media_stem("pub-x_X_1_VIDEO", "X", label);
        MediaEntry {
            id: 0,
            key: "pub-x_X_1_VIDEO".into(),
            lang_code: "X".into(),
            title: "Einführung".into(),
            kind: MediaKind::Video,
            label: label.into(),
            file_name: format!("{stem}.mp4"),
            subtitle_file: subtitles.then(|| format!("{stem}.vtt")),
            size: 4,
            duration: None,
            image: None,
            downloaded_at: 1,
        }
    }

    #[test]
    fn failed_captions_fall_back_to_the_earlier_download() {
        let tmp = tempfile::tempdir().unwrap();
        let lib = Library::open(tmp.path()).unwrap();
        let dir = lib.media_dir();
        fs::create_dir_all(&dir).unwrap();
        let (key, lang) = ("pub-x_X_1_VIDEO", "X");

        // Nothing earlier: no captions.
        let a = media_entry("480p", true);
        let a_vtt = a.subtitle_file.clone().unwrap();
        assert_eq!(earlier_captions(&lib, key, lang, &a_vtt), None);
        assert!(!dir.join(&a_vtt).exists());

        fs::write(dir.join(&a.file_name), "data").unwrap();
        fs::write(dir.join(&a_vtt), "WEBVTT").unwrap();
        lib.add_media(&a).unwrap();
        // The same quality again keeps the file it left.
        assert_eq!(
            earlier_captions(&lib, key, lang, &a_vtt),
            Some(a_vtt.clone())
        );

        // Another quality gets a copy under its own name, which survives
        // the earlier download's files being replaced.
        let mut b = media_entry("720p", false);
        let b_vtt = format!("{}.vtt", b.file_name.strip_suffix(".mp4").unwrap());
        b.subtitle_file = earlier_captions(&lib, key, lang, &b_vtt);
        assert_eq!(b.subtitle_file.as_deref(), Some(b_vtt.as_str()));
        fs::write(dir.join(&b.file_name), "data").unwrap();
        lib.add_media(&b).unwrap();
        assert_eq!(fs::read_to_string(dir.join(&b_vtt)).unwrap(), "WEBVTT");
        assert!(!dir.join(&a_vtt).exists());
        assert!(!dir.join(&a.file_name).exists());
    }

    #[test]
    fn saved_extensions() {
        assert_eq!(media_extension("video/mp4"), Some("mp4"));
        assert_eq!(media_extension("audio/mpeg"), Some("mp3"));
        assert_eq!(media_extension("text/html"), None);
    }
}
