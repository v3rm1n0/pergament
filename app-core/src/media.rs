//! Responses for the media URLs the UI loads: pictures (`jwmedia:`) and files in the media folder.

use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use crate::api;
use crate::core::Core;

/// Which URL a request came in on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaRoute {
    /// `jwmedia://localhost/<path>`: a publication image, or a catalog image fetched once and cached.
    Picture,
    /// `asset://localhost/<encoded absolute path>`: a recording or its captions in the media folder.
    File,
}

pub struct MediaResponse {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: MediaBody,
}

impl MediaResponse {
    /// The first header called `name`, compared case-insensitively.
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }

    fn empty(status: u16) -> MediaResponse {
        MediaResponse {
            status,
            headers: Vec::new(),
            body: MediaBody(None),
        }
    }
}

/// Yields exactly the bytes of the response body; empty for 404, 403 and 416.
pub struct MediaBody(Option<io::Take<File>>);

impl Read for MediaBody {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match &mut self.0 {
            Some(reader) => reader.read(buf),
            None => Ok(0),
        }
    }
}

/// Whether serving the picture at `path` requires a network fetch (an uncached remote image).
pub fn needs_fetch(core: &Core, path: &str) -> bool {
    api::remote_image(core.cache(), path).is_some_and(|(_, f)| !f.is_file())
}

/// The response for `path` on `route`, limited to the byte range in a `Range` header value.
/// Blocks while an uncached catalog picture is fetched.
pub fn media_response(
    core: &Core,
    route: MediaRoute,
    path: &str,
    range: Option<&str>,
) -> MediaResponse {
    let resolved = match route {
        MediaRoute::Picture => picture_file(core, path).ok_or(404),
        MediaRoute::File => media_folder_file(core, path),
    };
    let file = match resolved {
        Ok(file) => file,
        Err(status) => return MediaResponse::empty(status),
    };
    let Ok(mut opened) = File::open(&file) else {
        return MediaResponse::empty(404);
    };
    let Ok(len) = opened.metadata().map(|m| m.len()) else {
        return MediaResponse::empty(404);
    };
    let (status, start, end) = match slice(range, len) {
        Slice::Whole => (200, 0, len),
        Slice::Part(start, last) => (206, start, last + 1),
        Slice::Unsatisfiable => {
            let mut response = MediaResponse::empty(416);
            response
                .headers
                .push(("Content-Range".to_owned(), format!("bytes */{len}")));
            return response;
        }
    };
    if start > 0 && opened.seek(SeekFrom::Start(start)).is_err() {
        return MediaResponse::empty(404);
    }
    let mime = match route {
        MediaRoute::Picture => api::mime_for(&file),
        MediaRoute::File => file_mime(&file),
    };
    let mut headers = vec![
        ("Content-Type".to_owned(), mime.to_owned()),
        ("Content-Length".to_owned(), (end - start).to_string()),
        ("Accept-Ranges".to_owned(), "bytes".to_owned()),
    ];
    if status == 206 {
        headers.push((
            "Content-Range".to_owned(),
            format!("bytes {start}-{}/{len}", end - 1),
        ));
    }
    if route == MediaRoute::Picture {
        headers.push(("Cache-Control".to_owned(), "max-age=3600".to_owned()));
    }
    MediaResponse {
        status,
        headers,
        body: MediaBody(Some(opened.take(end - start))),
    }
}

/// The part of a file a `Range` header asks for.
#[derive(Debug, PartialEq, Eq)]
enum Slice {
    Whole,
    /// First and last byte, both inside the file.
    Part(u64, u64),
    Unsatisfiable,
}

/// Malformed and multi-range headers are ignored, so the whole file is sent (RFC 9110 14.2).
fn slice(range: Option<&str>, len: u64) -> Slice {
    let Some(spec) = range.map(str::trim).and_then(|r| {
        r.get(..6)
            .filter(|unit| unit.eq_ignore_ascii_case("bytes="))
            .map(|_| &r[6..])
    }) else {
        return Slice::Whole;
    };
    let Some((first, last)) = spec.split_once('-') else {
        return Slice::Whole;
    };
    if spec.contains(',') {
        return Slice::Whole;
    }
    let (first, last) = (first.trim(), last.trim());
    match (digits(first), digits(last)) {
        (None, Some(suffix)) if first.is_empty() => {
            if suffix == 0 || len == 0 {
                Slice::Unsatisfiable
            } else {
                Slice::Part(len.saturating_sub(suffix), len - 1)
            }
        }
        (Some(start), end) if end.is_some() || last.is_empty() => {
            let end = end.unwrap_or(u64::MAX);
            if end < start {
                Slice::Whole
            } else if start >= len {
                Slice::Unsatisfiable
            } else {
                Slice::Part(start, end.min(len - 1))
            }
        }
        _ => Slice::Whole,
    }
}

/// A number made of ASCII digits only (`u64::from_str` would also take a sign).
fn digits(s: &str) -> Option<u64> {
    (!s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()))
        .then(|| s.parse().ok())
        .flatten()
}

/// The file behind `/<encodeURIComponent(absolute path)>`, which must lie in the media folder.
fn media_folder_file(core: &Core, path: &str) -> Result<PathBuf, u16> {
    let decoded = percent_decode(path.strip_prefix('/').unwrap_or(path)).ok_or(404u16)?;
    let requested = Path::new(&decoded);
    if !requested.is_absolute() {
        return Err(403);
    }
    let folder = core
        .media_dir()
        .ok()
        .and_then(|dir| dir.canonicalize().ok())
        .ok_or(404u16)?;
    let file = requested.canonicalize().map_err(|_| 404u16)?;
    if !file.starts_with(&folder) {
        return Err(403);
    }
    file.is_file().then_some(file).ok_or(404)
}

fn percent_decode(s: &str) -> Option<String> {
    let mut bytes = Vec::with_capacity(s.len());
    let mut rest = s.as_bytes();
    while let Some((&b, tail)) = rest.split_first() {
        if b == b'%' {
            let hex = std::str::from_utf8(tail.get(..2)?).ok()?;
            bytes.push(u8::from_str_radix(hex, 16).ok()?);
            rest = &tail[2..];
        } else {
            bytes.push(b);
            rest = tail;
        }
    }
    String::from_utf8(bytes).ok()
}

/// Recordings and their captions; anything else is typed like a picture.
fn file_mime(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("mp4" | "m4v") => "video/mp4",
        Some("mp3") => "audio/mpeg",
        Some("m4a") => "audio/mp4",
        Some("vtt") => "text/vtt",
        _ => api::mime_for(path),
    }
}

/// Catalog and media images are fetched once (rate limited) and cached; publication
/// images are read from the library.
fn picture_file(core: &Core, path: &str) -> Option<PathBuf> {
    match api::remote_image(core.cache(), path) {
        Some((url, cached)) => {
            if !cached.is_file() {
                let fetched = core.image_client().lock().ok().and_then(|client| {
                    let mut ignore = |_: u64, _: Option<u64>| {};
                    client
                        .download(&url, &cached, &Default::default(), &mut ignore)
                        .ok()
                });
                if fetched.is_none() {
                    let _ = std::fs::remove_file(pergament::net::part_path(&cached));
                }
            }
            cached.is_file().then_some(cached)
        }
        None => core
            .lib()
            .ok()
            .and_then(|lib| api::media_file(&lib, &core.root().join("publications"), path)),
    }
}

#[cfg(test)]
mod tests {
    use std::io::Read;
    use std::sync::Arc;

    use super::*;
    use crate::core::{Host, Paths};
    use crate::dispatch::common;

    struct NoHost;

    impl Host for NoHost {
        fn emit(&self, _event: &str, _payload: serde_json::Value) {}
    }

    struct Env {
        core: Core,
        _library: tempfile::TempDir,
        cache: tempfile::TempDir,
    }

    fn env() -> Env {
        let library = tempfile::tempdir().unwrap();
        let cache = tempfile::tempdir().unwrap();
        let paths = Paths {
            library: library.path().to_owned(),
            cache: cache.path().to_owned(),
        };
        Env {
            core: Core::open(paths, Arc::new(NoHost)).unwrap(),
            _library: library,
            cache,
        }
    }

    fn header<'a>(response: &'a MediaResponse, name: &str) -> Option<&'a str> {
        response.header(name)
    }

    fn body(mut response: MediaResponse) -> Vec<u8> {
        let mut bytes = Vec::new();
        response.body.read_to_end(&mut bytes).unwrap();
        bytes
    }

    #[test]
    fn unknown_picture_path_is_404() {
        let env = env();
        let response = media_response(&env.core, MediaRoute::Picture, "/nope/img_1.jpg", None);
        assert_eq!(response.status, 404);
        assert!(!needs_fetch(&env.core, "/nope/img_1.jpg"));
        assert!(body(response).is_empty());
    }

    #[test]
    fn library_picture_is_200_with_its_type() {
        let env = env();
        let file = env.cache.path().join("tst_E.jwpub");
        common::Fixture::default().write_to(&file);
        env.core
            .import_files(vec![file.to_str().unwrap().to_owned()])
            .unwrap();
        let dir = env.core.lib().unwrap().list().unwrap()[0].dir_name.clone();
        let path = format!("/{dir}/img_1.jpg");
        assert!(!needs_fetch(&env.core, &path));
        let response = media_response(&env.core, MediaRoute::Picture, &path, None);
        assert_eq!(response.status, 200);
        assert_eq!(header(&response, "Content-Type"), Some("image/jpeg"));
        assert_eq!(header(&response, "Cache-Control"), Some("max-age=3600"));
        assert_eq!(header(&response, "Accept-Ranges"), Some("bytes"));
        assert_eq!(body(response), b"\xff\xd8not really a jpeg");
    }

    /// The URL path the shim's `convertFileSrc` builds: `/` + encodeURIComponent(absolute path).
    fn url_path(path: &Path) -> String {
        let mut out = String::from("/");
        for b in path.to_str().unwrap().bytes() {
            if b.is_ascii_alphanumeric() || b"-_.!~*'()".contains(&b) {
                out.push(b as char);
            } else {
                out.push_str(&format!("%{b:02X}"));
            }
        }
        out
    }

    /// A core whose media folder holds `name` with the bytes 0..10.
    fn with_media(name: &str) -> (Env, String) {
        let env = env();
        let file = env.core.media_dir().unwrap().join(name);
        std::fs::write(&file, (0u8..10).collect::<Vec<_>>()).unwrap();
        let path = url_path(&file);
        (env, path)
    }

    fn file(env: &Env, path: &str, range: Option<&str>) -> MediaResponse {
        media_response(&env.core, MediaRoute::File, path, range)
    }

    #[test]
    fn range_returns_206_and_content_range() {
        let (env, path) = with_media("a.mp4");
        let response = file(&env, &path, Some("bytes=2-4"));
        assert_eq!(response.status, 206);
        assert_eq!(header(&response, "Content-Range"), Some("bytes 2-4/10"));
        assert_eq!(header(&response, "Content-Length"), Some("3"));
        assert_eq!(header(&response, "Content-Type"), Some("video/mp4"));
        assert_eq!(header(&response, "Accept-Ranges"), Some("bytes"));
        assert_eq!(body(response), [2, 3, 4]);
    }

    #[test]
    fn open_ended_range() {
        let (env, path) = with_media("a.mp4");
        let response = file(&env, &path, Some("bytes=7-"));
        assert_eq!(response.status, 206);
        assert_eq!(header(&response, "Content-Range"), Some("bytes 7-9/10"));
        assert_eq!(body(response).len(), 3);
    }

    #[test]
    fn end_past_the_file_is_clamped() {
        let (env, path) = with_media("a.mp4");
        let response = file(&env, &path, Some("bytes=8-100"));
        assert_eq!(response.status, 206);
        assert_eq!(header(&response, "Content-Range"), Some("bytes 8-9/10"));
        assert_eq!(body(response), [8, 9]);
    }

    #[test]
    fn suffix_range_is_the_last_bytes() {
        let (env, path) = with_media("a.mp4");
        let response = file(&env, &path, Some("bytes=-4"));
        assert_eq!(response.status, 206);
        assert_eq!(header(&response, "Content-Range"), Some("bytes 6-9/10"));
        assert_eq!(body(response), [6, 7, 8, 9]);
        let longer = file(&env, &path, Some("bytes=-50"));
        assert_eq!(header(&longer, "Content-Range"), Some("bytes 0-9/10"));
        assert_eq!(body(longer).len(), 10);
    }

    #[test]
    fn unsatisfiable_range_is_416() {
        let (env, path) = with_media("a.mp4");
        let response = file(&env, &path, Some("bytes=20-30"));
        assert_eq!(response.status, 416);
        assert_eq!(header(&response, "Content-Range"), Some("bytes */10"));
        assert!(body(response).is_empty());
        assert_eq!(file(&env, &path, Some("bytes=10-")).status, 416);
        assert_eq!(file(&env, &path, Some("bytes=-0")).status, 416);
    }

    #[test]
    fn no_range_is_200_full_body() {
        let (env, path) = with_media("a.mp4");
        let response = file(&env, &path, None);
        assert_eq!(response.status, 200);
        assert_eq!(header(&response, "Content-Length"), Some("10"));
        assert_eq!(header(&response, "Accept-Ranges"), Some("bytes"));
        assert_eq!(header(&response, "Content-Range"), None);
        assert_eq!(body(response), (0u8..10).collect::<Vec<_>>());
    }

    #[test]
    fn malformed_or_multiple_ranges_are_ignored() {
        let (env, path) = with_media("a.mp4");
        for range in [
            "bytes=4-2",
            "bytes=x-3",
            "bytes=-",
            "items=0-3",
            "bytes=0-1,4-5",
            "",
        ] {
            let response = file(&env, &path, Some(range));
            assert_eq!(response.status, 200, "{range}");
            assert_eq!(body(response).len(), 10, "{range}");
        }
    }

    #[test]
    fn subtitles_are_webvtt() {
        let (env, path) = with_media("a.vtt");
        let response = file(&env, &path, None);
        assert_eq!(header(&response, "Content-Type"), Some("text/vtt"));
    }

    #[test]
    fn a_file_outside_the_media_folder_is_403() {
        let (env, _) = with_media("a.mp4");
        let outside = env.cache.path().join("secret.txt");
        std::fs::write(&outside, b"no").unwrap();
        let response = file(&env, &url_path(&outside), None);
        assert_eq!(response.status, 403);
        assert!(body(response).is_empty());

        // Starts with the media folder, so only the canonical path shows it is outside.
        let media = env.core.media_dir().unwrap();
        std::fs::write(media.join("..").join("beside.txt"), b"no").unwrap();
        let through_dots = media.join("..").join("beside.txt");
        assert_eq!(file(&env, &url_path(&through_dots), None).status, 403);
    }

    #[test]
    fn a_missing_file_is_404() {
        let (env, _) = with_media("a.mp4");
        let missing = env.core.media_dir().unwrap().join("gone.mp4");
        assert_eq!(file(&env, &url_path(&missing), None).status, 404);
        let folder = env.core.media_dir().unwrap();
        assert_eq!(file(&env, &url_path(&folder), None).status, 404);
        assert_eq!(file(&env, "/%ZZ", None).status, 404);
    }

    #[test]
    fn a_percent_encoded_name_is_decoded() {
        let (env, path) = with_media("Psalm 23 – Grüße.mp3");
        assert!(path.contains("%20") && path.contains("%C3%BC"), "{path}");
        let response = file(&env, &path, Some("bytes=0-0"));
        assert_eq!(response.status, 206);
        assert_eq!(header(&response, "Content-Type"), Some("audio/mpeg"));
        assert_eq!(body(response), [0]);
    }
}
