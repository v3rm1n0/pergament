//! Plain functions behind the Tauri commands, so they can be tested without
//! a running app. Everything returned is serialized for the frontend.

use std::path::{Path, PathBuf};

use jwlinux::links::Link;
use jwlinux::navigate::{self, Page, Target};
use jwlinux::reader::{BibleBook, TocNode};
use jwlinux::render::media_name;
use jwlinux::{Entry, Library, Publication};
use serde::Serialize;

pub type ApiResult<T> = Result<T, String>;

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

/// URL scheme serving publication images, see [`media_file`].
pub const MEDIA_SCHEME: &str = "jwmedia";

pub fn media_base(dir: &str) -> String {
    format!("{MEDIA_SCHEME}://localhost/{dir}/")
}

/// A publication as shown in the library.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PubCard {
    pub dir: String,
    pub title: String,
    pub short_title: Option<String>,
    pub symbol: String,
    pub year: i64,
    pub issue_tag: String,
    pub meps_language: i64,
    pub publication_type: Option<String>,
    pub is_bible: bool,
    pub cover: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PubDetail {
    pub card: PubCard,
    pub toc: Vec<TocNode>,
    pub books: Vec<BibleBook>,
}

fn card(entry: &Entry, publication: &Publication) -> ApiResult<PubCard> {
    Ok(PubCard {
        dir: entry.dir_name.clone(),
        title: entry.title.clone(),
        short_title: entry.short_title.clone(),
        symbol: entry.symbol.clone(),
        year: entry.year,
        issue_tag: entry.issue_tag.clone(),
        meps_language: entry.meps_language,
        publication_type: entry.publication_type.clone(),
        is_bible: publication.is_bible(),
        cover: publication
            .cover_image()
            .map_err(err)?
            .map(|f| format!("{}{f}", media_base(&entry.dir_name))),
    })
}

fn open_entry(library: &Library, dir: &str) -> ApiResult<(Entry, Publication)> {
    let entry = library
        .get_by_dir(dir)
        .map_err(err)?
        .ok_or_else(|| format!("{dir} is not in the library"))?;
    let publication = Publication::open(library, &entry).map_err(err)?;
    Ok((entry, publication))
}

pub fn list_publications(library: &Library) -> ApiResult<Vec<PubCard>> {
    library
        .list()
        .map_err(err)?
        .iter()
        .map(|e| card(e, &Publication::open(library, e).map_err(err)?))
        .collect()
}

pub fn publication(library: &Library, dir: &str) -> ApiResult<PubDetail> {
    let (entry, publication) = open_entry(library, dir)?;
    Ok(PubDetail {
        card: card(&entry, &publication)?,
        toc: publication.toc().map_err(err)?,
        books: if publication.is_bible() {
            publication.bible_books().map_err(err)?
        } else {
            Vec::new()
        },
    })
}

/// Render a page as an HTML fragment with images served via [`MEDIA_SCHEME`].
pub fn render(library: &Library, target: &Target) -> ApiResult<Page> {
    navigate::render(library, target, false, |e, _| Some(media_base(&e.dir_name))).map_err(err)
}

/// What the frontend should do with a clicked link.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum LinkAction {
    /// `study_note`: show the study note of the target verse.
    Open {
        target: Target,
        #[serde(rename = "studyNote")]
        study_note: bool,
    },
    External {
        url: String,
    },
    /// Not in the library; `url` opens it on jw.org.
    Missing {
        url: Option<String>,
    },
    Ignore,
}

pub fn link_action(library: &Library, current: Option<&str>, href: &str) -> ApiResult<LinkAction> {
    let Some(link) = Link::parse(href) else {
        return Ok(LinkAction::Ignore);
    };
    let current = match current {
        Some(dir) => library.get_by_dir(dir).map_err(err)?,
        None => None,
    };
    Ok(match &link {
        Link::External(url) => LinkAction::External { url: url.clone() },
        Link::Fragment(_) => LinkAction::Ignore,
        _ => match navigate::resolve(library, current.as_ref(), &link).map_err(err)? {
            Some(target) => LinkAction::Open {
                target,
                study_note: matches!(link, Link::StudyNote { .. }),
            },
            None => LinkAction::Missing {
                url: match &link {
                    Link::Document {
                        lang_code,
                        meps_document_id,
                    } => Some(format!(
                        "https://www.jw.org/finder?wtlocale={lang_code}&docid={meps_document_id}"
                    )),
                    _ => None,
                },
            },
        },
    })
}

/// Map a media request path `/{dir}/{file}` to a file in the library, or
/// `None` if it is not a plain image name inside an imported publication.
pub fn media_file(library: &Library, publications_root: &Path, path: &str) -> Option<PathBuf> {
    let mut parts = path.trim_start_matches('/').splitn(2, '/');
    let (dir, file) = (parts.next()?, parts.next()?);
    let src = format!("jwpub-media://{file}");
    let file = media_name(&src)?;
    let entry = library.get_by_dir(dir).ok()??;
    let full = publications_root.join(&entry.dir_name).join(file);
    full.is_file().then_some(full)
}

pub fn mime_for(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("png") => "image/png",
        Some("gif") => "image/gif",
        Some("svg") => "image/svg+xml",
        Some("webp") => "image/webp",
        _ => "application/octet-stream",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn link_actions_without_library_entries() {
        let tmp = tempfile::tempdir().unwrap();
        let lib = Library::open(tmp.path()).unwrap();
        assert_eq!(
            link_action(&lib, None, "https://www.jw.org/").unwrap(),
            LinkAction::External {
                url: "https://www.jw.org/".into()
            }
        );
        assert_eq!(
            link_action(&lib, None, "jwlinux://pub/X:2026003/").unwrap(),
            LinkAction::Missing {
                url: Some("https://www.jw.org/finder?wtlocale=X&docid=2026003".into())
            }
        );
        assert_eq!(
            link_action(&lib, None, "jwlinux://bible/19:23:1").unwrap(),
            LinkAction::Missing { url: None }
        );
        assert_eq!(
            link_action(&lib, None, "javascript:alert(1)").unwrap(),
            LinkAction::Ignore
        );
        assert_eq!(link_action(&lib, None, "#x").unwrap(), LinkAction::Ignore);
    }

    #[test]
    fn media_paths_are_confined() {
        let tmp = tempfile::tempdir().unwrap();
        let lib = Library::open(tmp.path()).unwrap();
        let root = tmp.path().join("publications");
        // Nothing is imported, so even plausible paths are refused.
        for p in [
            "/nwtsty_2/a.jpg",
            "/../index.sqlite",
            "/nwtsty_2/../../index.sqlite",
            "/nwtsty_2/",
            "",
        ] {
            assert_eq!(media_file(&lib, &root, p), None, "{p}");
        }
    }

    #[test]
    fn mime_types() {
        assert_eq!(mime_for(Path::new("a.JPG")), "image/jpeg");
        assert_eq!(mime_for(Path::new("a.png")), "image/png");
        assert_eq!(mime_for(Path::new("a")), "application/octet-stream");
    }
}
