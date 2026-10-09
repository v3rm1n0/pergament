//! Fetching publications: look up the file with the pub-media API, download
//! it with verification, then import it like any user-supplied file.

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use serde::Deserialize;

use crate::catalog::CatalogItem;
use crate::library::{Entry, Library};
use crate::links::{MediaKind, MediaRef};
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
}

#[derive(Debug, Deserialize)]
struct ImageRef {
    #[serde(default)]
    url: String,
}

/// One playable rendition of a recording.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
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
    let result = library.import(&dest);
    let _ = fs::remove_file(&dest);
    let entry = result?;

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
    Ok(entry)
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
