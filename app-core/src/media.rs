//! Responses for the media URLs the UI loads: pictures (`jwmedia:`) and files in the media folder.

use std::fs::File;
use std::io::{self, Read};
use std::path::PathBuf;

use crate::api;
use crate::core::Core;

/// Which URL a request came in on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaRoute {
    /// `jwmedia://localhost/<path>`: a publication image, or a catalog image fetched once and cached.
    Picture,
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

/// The response for `path` on `route`. Blocks while an uncached catalog picture is fetched.
pub fn media_response(core: &Core, route: MediaRoute, path: &str) -> MediaResponse {
    let file = match route {
        MediaRoute::Picture => picture_file(core, path),
    };
    let Some(file) = file else {
        return MediaResponse::empty(404);
    };
    let Ok(opened) = File::open(&file) else {
        return MediaResponse::empty(404);
    };
    let Ok(len) = opened.metadata().map(|m| m.len()) else {
        return MediaResponse::empty(404);
    };
    MediaResponse {
        status: 200,
        headers: vec![
            ("Content-Type".to_owned(), api::mime_for(&file).to_owned()),
            ("Content-Length".to_owned(), len.to_string()),
            ("Cache-Control".to_owned(), "max-age=3600".to_owned()),
        ],
        body: MediaBody(Some(opened.take(len))),
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
        let response = media_response(&env.core, MediaRoute::Picture, "/nope/img_1.jpg");
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
        let response = media_response(&env.core, MediaRoute::Picture, &path);
        assert_eq!(response.status, 200);
        assert_eq!(header(&response, "Content-Type"), Some("image/jpeg"));
        assert_eq!(header(&response, "Cache-Control"), Some("max-age=3600"));
        assert_eq!(body(response), b"\xff\xd8not really a jpeg");
    }
}
