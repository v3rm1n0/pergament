//! The public media catalog (`apis/mediator/v1/categories`): video and audio
//! categories with their recordings, cached locally. See docs/FORMAT.md
//! ("Media catalog").

use std::collections::HashMap;
use std::fs::{self, File};
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use serde::{Deserialize, Serialize};

use crate::links::MediaKind;
use crate::net::Client;
use crate::remote::MediaFile;
use crate::{Error, Result};

pub const MEDIATOR_URL: &str = "https://b.jw-cdn.org/apis/mediator/v1/categories";

/// Base URL of the images in the catalog; [`Media::image`] is relative to it.
pub const IMAGE_BASE: &str = "https://cms-imgp.jw-cdn.org/img/";

/// Top-level category keys.
pub const VIDEO_ROOT: &str = "VideoOnDemand";
pub const AUDIO_ROOT: &str = "Audio";

/// A category: either a container of subcategories or a list of recordings.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Category {
    pub key: String,
    pub name: String,
    /// Holds subcategories rather than recordings.
    pub container: bool,
    /// Image path below [`IMAGE_BASE`] (or a URL once mapped by the app).
    pub image: Option<String>,
    pub subcategories: Vec<Category>,
    pub media: Vec<Media>,
}

/// One recording with its renditions, smallest first.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Media {
    /// `naturalKey` such as `pub-nwtsv_X_1_VIDEO`; unique per language.
    pub key: String,
    pub title: String,
    pub kind: MediaKind,
    /// Seconds.
    pub duration: Option<f64>,
    /// RFC 3339.
    pub published: Option<String>,
    /// Thumbnail path below [`IMAGE_BASE`] (or a URL once mapped by the app).
    pub image: Option<String>,
    pub files: Vec<MediaFile>,
}

impl Category {
    /// Replace every image path, e.g. with a URL the frontend can load.
    pub fn map_images(&mut self, f: &dyn Fn(&str) -> String) {
        if let Some(i) = &mut self.image {
            *i = f(i);
        }
        for m in &mut self.media {
            if let Some(i) = &mut m.image {
                *i = f(i);
            }
            for file in &mut m.files {
                if let Some(i) = &mut file.poster {
                    *i = f(i);
                }
            }
        }
        for c in &mut self.subcategories {
            c.map_images(f);
        }
    }

    /// The recording with this key, in this category or below.
    pub fn find(&self, key: &str) -> Option<&Media> {
        self.media
            .iter()
            .find(|m| m.key == key)
            .or_else(|| self.subcategories.iter().find_map(|c| c.find(key)))
    }
}

#[derive(Deserialize)]
struct Response {
    category: RawCategory,
}

#[derive(Deserialize)]
struct RawCategory {
    key: String,
    #[serde(default, deserialize_with = "null_default")]
    name: String,
    #[serde(rename = "type", default, deserialize_with = "null_default")]
    kind: String,
    #[serde(default)]
    images: Images,
    #[serde(default)]
    subcategories: Vec<RawCategory>,
    #[serde(default)]
    media: Vec<RawMedia>,
}

/// A missing or `null` value becomes the default.
fn null_default<'de, D, T>(d: D) -> std::result::Result<T, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Default + Deserialize<'de>,
{
    Ok(Option::<T>::deserialize(d)?.unwrap_or_default())
}

/// `{"wss": {"sm": url, "lg": url}, "sqr": {…}}`
type Images = HashMap<String, HashMap<String, String>>;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawMedia {
    natural_key: String,
    #[serde(default, deserialize_with = "null_default")]
    title: String,
    #[serde(rename = "type", default, deserialize_with = "null_default")]
    kind: String,
    #[serde(default)]
    duration: Option<f64>,
    #[serde(default)]
    first_published: Option<String>,
    #[serde(default)]
    images: Images,
    #[serde(default)]
    files: Vec<RawFile>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawFile {
    #[serde(rename = "progressiveDownloadURL")]
    url: String,
    #[serde(default)]
    checksum: Option<String>,
    #[serde(default)]
    filesize: Option<u64>,
    #[serde(default)]
    frame_height: Option<u32>,
    #[serde(default, deserialize_with = "null_default")]
    label: String,
    #[serde(default, deserialize_with = "null_default")]
    mimetype: String,
    #[serde(default)]
    duration: Option<f64>,
    #[serde(default)]
    subtitles: Option<RawSubtitles>,
}

#[derive(Deserialize)]
struct RawSubtitles {
    #[serde(default)]
    url: String,
}

/// Image kinds to try, best first. Wide thumbnails for video, square for audio.
const WIDE: &[&str] = &["wss", "lsr", "wsr", "pnr", "sqr", "sqs"];
const SQUARE: &[&str] = &["sqr", "sqs", "wss", "lsr", "wsr", "pnr"];
/// Sizes to try, smallest useful first.
const SIZES: &[&str] = &["sm", "md", "lg", "xl"];

/// A path below [`IMAGE_BASE`] with only safe characters.
pub fn safe_image_path(path: &str) -> bool {
    let parts: Vec<&str> = path.split('/').collect();
    (2..=8).contains(&parts.len())
        && parts.iter().all(|p| {
            !p.is_empty()
                && !p.starts_with('.')
                && p.chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
        })
        && parts.last().is_some_and(|f| {
            let ext = f
                .rsplit('.')
                .next()
                .unwrap_or_default()
                .to_ascii_lowercase();
            matches!(ext.as_str(), "jpg" | "jpeg" | "png" | "webp")
        })
}

fn pick_image(images: &Images, kinds: &[&str]) -> Option<String> {
    kinds.iter().find_map(|k| {
        let sizes = images.get(*k)?;
        SIZES
            .iter()
            .find_map(|s| sizes.get(*s))
            .or_else(|| sizes.values().next())
            .and_then(|url| url.strip_prefix(IMAGE_BASE))
            .filter(|rel| safe_image_path(rel))
            .map(str::to_owned)
    })
}

fn convert_media(raw: RawMedia, client: &Client) -> Option<Media> {
    let kind = match raw.kind.as_str() {
        "video" => MediaKind::Video,
        "audio" => MediaKind::Audio,
        _ => return None,
    };
    let prefix = match kind {
        MediaKind::Video => "video/",
        MediaKind::Audio => "audio/",
    };
    let image = pick_image(
        &raw.images,
        if kind == MediaKind::Video {
            WIDE
        } else {
            SQUARE
        },
    );
    let mut files: Vec<(u32, MediaFile)> = raw
        .files
        .into_iter()
        .filter(|f| f.mimetype.starts_with(prefix) && client.check_url(&f.url).is_ok())
        .map(|f| {
            let subtitles = f
                .subtitles
                .map(|s| s.url)
                .filter(|u| !u.is_empty() && client.check_url(u).is_ok());
            (
                f.frame_height.unwrap_or(0),
                MediaFile {
                    title: raw.title.clone(),
                    label: f.label,
                    mime: f.mimetype,
                    url: f.url,
                    size: f.filesize,
                    duration: f.duration.or(raw.duration),
                    poster: image.clone(),
                    subtitles,
                    md5: f.checksum.filter(|c| !c.is_empty()),
                },
            )
        })
        .collect();
    if files.is_empty() {
        return None;
    }
    files.sort_by_key(|(height, f)| (*height, f.size));
    Some(Media {
        key: raw.natural_key,
        title: raw.title,
        kind,
        duration: raw.duration,
        published: raw.first_published,
        image,
        files: files.into_iter().map(|(_, f)| f).collect(),
    })
}

fn convert(raw: RawCategory, client: &Client) -> Category {
    Category {
        container: raw.kind == "container",
        image: pick_image(&raw.images, WIDE),
        subcategories: raw
            .subcategories
            .into_iter()
            .map(|c| convert(c, client))
            .collect(),
        media: raw
            .media
            .into_iter()
            .filter_map(|m| convert_media(m, client))
            .collect(),
        key: raw.key,
        name: raw.name,
    }
}

/// Category keys and language codes become part of a URL and a file name.
pub fn valid_key(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 64
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'))
}

/// Parse a mediator response. Recordings on hosts the client does not allow
/// are dropped.
pub fn parse(reader: impl std::io::Read, client: &Client) -> Result<Category> {
    let resp: Response = serde_json::from_reader(reader)?;
    Ok(convert(resp.category, client))
}

fn cache_file(cache: &Path, lang: &str, key: &str, detailed: bool) -> PathBuf {
    cache
        .join("mediator")
        .join(format!("{lang}_{key}_{}.json", u8::from(detailed)))
}

fn fresh(path: &Path, ttl: Duration) -> bool {
    fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| SystemTime::now().duration_since(t).ok())
        .is_some_and(|age| age < ttl)
}

/// A category in a language (`X`, `E`, …). `detailed` includes the recordings
/// of subcategories, which the top-level lists leave out to stay small. The
/// response is cached; a stale copy is used when the network fails.
pub fn category(
    client: &Client,
    cache: &Path,
    lang: &str,
    key: &str,
    detailed: bool,
    ttl: Duration,
) -> Result<Category> {
    if !valid_key(lang) || !valid_key(key) {
        return Err(Error::NotFound(format!("invalid category {lang}/{key}")));
    }
    let file = cache_file(cache, lang, key, detailed);
    if !fresh(&file, ttl) {
        let url = format!(
            "{MEDIATOR_URL}/{lang}/{key}?detailed={}",
            u8::from(detailed)
        );
        let mut ignore = |_: u64, _: Option<u64>| {};
        if let Err(e) = client.download(&url, &file, &Default::default(), &mut ignore) {
            let _ = fs::remove_file(crate::net::part_path(&file));
            if !file.is_file() {
                return Err(match e {
                    Error::Http(m) if m.contains("HTTP 404") => {
                        Error::NotFound(format!("category {key} in {lang}"))
                    }
                    e => e,
                });
            }
        }
    }
    parse(BufReader::new(File::open(&file)?), client)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::net::HttpConfig;

    const SAMPLE: &str = r#"{"category":{"key":"VODBible","name":"Die Bibel","type":"container",
      "images":{"wss":{"sm":"https://cms-imgp.jw-cdn.org/img/p/x/univ/art/x_wss_sm.jpg"}},
      "subcategories":[{"key":"BibleBooks","name":"Bibelbücher","type":"ondemand","media":[
        {"naturalKey":"pub-nwtsv_X_1_VIDEO","type":"video","title":"Einführung","duration":327.7,
         "firstPublished":"2023-06-30T15:42:03.508Z",
         "images":{"pnr":{"lg":"https://cms-imgp.jw-cdn.org/img/p/y/pnr_lg.jpg"},
                   "wss":{"lg":"https://cms-imgp.jw-cdn.org/img/p/y/wss_lg.jpg","sm":"https://cms-imgp.jw-cdn.org/img/p/y/wss_sm.jpg"}},
         "files":[
          {"progressiveDownloadURL":"https://cfp2.jw-cdn.org/a/2/nwtsv_X_001_r480P.mp4","checksum":"b","filesize":34,
           "frameHeight":540,"label":"480p","mimetype":"video/mp4","duration":327.7,
           "subtitles":{"url":"https://cfp2.jw-cdn.org/a/1/nwtsv_X_001.vtt"}},
          {"progressiveDownloadURL":"https://cfp2.jw-cdn.org/a/1/nwtsv_X_001_r240P.mp4","checksum":"a","filesize":8,
           "frameHeight":234,"label":"240p","mimetype":"video/mp4"},
          {"progressiveDownloadURL":"https://evil.example/x.mp4","label":"720p","mimetype":"video/mp4","frameHeight":720}]},
        {"naturalKey":"pub-x_X_1_AUDIO","type":"audio","title":"Lied","images":{},
         "files":[{"progressiveDownloadURL":"https://cfp2.jw-cdn.org/a/3/x.mp3","label":null,"mimetype":"audio/mpeg"}]},
        {"naturalKey":"odd","type":"other","files":[]},
        {"naturalKey":"nofiles","type":"video","files":[]}]}]}}"#;

    #[test]
    fn parses_catalog() {
        let client = Client::new(HttpConfig::default());
        let c = parse(SAMPLE.as_bytes(), &client).unwrap();
        assert!(c.container);
        assert_eq!(c.image.as_deref(), Some("p/x/univ/art/x_wss_sm.jpg"));
        let sub = &c.subcategories[0];
        assert!(!sub.container);
        assert_eq!(sub.media.len(), 2);
        let v = &sub.media[0];
        assert_eq!(v.kind, MediaKind::Video);
        assert_eq!(v.image.as_deref(), Some("p/y/wss_sm.jpg"));
        // Smallest first, and the file on a foreign host is dropped.
        let labels: Vec<_> = v.files.iter().map(|f| f.label.as_str()).collect();
        assert_eq!(labels, ["240p", "480p"]);
        assert_eq!(
            v.files[1].subtitles.as_deref(),
            Some("https://cfp2.jw-cdn.org/a/1/nwtsv_X_001.vtt")
        );
        assert_eq!(v.files[0].md5.as_deref(), Some("a"));
        assert_eq!(sub.media[1].kind, MediaKind::Audio);
        assert!(c.find("pub-x_X_1_AUDIO").is_some());
        assert!(c.find("nofiles").is_none());
    }

    #[test]
    fn maps_images() {
        let client = Client::new(HttpConfig::default());
        let mut c = parse(SAMPLE.as_bytes(), &client).unwrap();
        c.map_images(&|p| format!("jwmedia://localhost/cms/{p}"));
        assert_eq!(
            c.subcategories[0].media[0].files[0].poster.as_deref(),
            Some("jwmedia://localhost/cms/p/y/wss_sm.jpg")
        );
    }

    #[test]
    fn image_paths() {
        assert!(safe_image_path(
            "p/jwb-141/univ/art/jwb-141_univ_wss_03_sm.jpg"
        ));
        for bad in [
            "x.jpg",
            "../a/b.jpg",
            "a/.b/c.jpg",
            "a//b.jpg",
            "a/b.php",
            "a/b c/d.jpg",
            "a/b/c/d/e/f/g/h/i.jpg",
        ] {
            assert!(!safe_image_path(bad), "{bad}");
        }
    }

    #[test]
    fn keys() {
        assert!(valid_key("VODBible"));
        assert!(valid_key("X"));
        assert!(!valid_key(""));
        assert!(!valid_key("a/b"));
        assert!(!valid_key("../x"));
    }

    #[test]
    fn rejects_bad_keys_before_any_request() {
        let client = Client::new(HttpConfig::default());
        let tmp = tempfile::tempdir().unwrap();
        assert!(category(&client, tmp.path(), "X", "a/b", false, Duration::ZERO).is_err());
    }
}
