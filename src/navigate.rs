//! Resolving links and building pages for a viewer.

use crate::Result;
use crate::library::{Entry, Library};
use crate::links::Link;
use crate::reader::Publication;
use crate::render::{RenderOptions, Renderer, file_url_for_dir};

/// Something a viewer can show.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Target {
    /// `Entry::dir_name` of the publication.
    pub publication: String,
    pub kind: TargetKind,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TargetKind {
    Document(i64),
    Chapter { book: i64, chapter: i64, verse: i64 },
}

/// A rendered page.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Page {
    pub title: String,
    pub html: String,
    /// Element id to scroll to, if any.
    pub fragment: Option<String>,
    /// Stable, filesystem-safe name for this page.
    pub name: String,
}

/// A Bible in the library, preferring `language`, then the study edition.
pub fn find_bible(library: &Library, language: Option<i64>) -> Result<Option<Entry>> {
    let mut best: Option<(u8, Entry)> = None;
    for e in library.list()? {
        if !Publication::open(library, &e)?.is_bible() {
            continue;
        }
        let score =
            u8::from(Some(e.meps_language) == language) * 2 + u8::from(e.symbol == "nwtsty");
        if best.as_ref().is_none_or(|(s, _)| score > *s) {
            best = Some((score, e));
        }
    }
    Ok(best.map(|(_, e)| e))
}

/// Find a document by MEPS id, searching `current` first, then publications
/// in the same language, then all others.
pub fn find_document(
    library: &Library,
    meps_document_id: i64,
    current: Option<&Entry>,
) -> Result<Option<(Entry, i64)>> {
    let mut entries = library.list()?;
    let lang = current.map(|c| c.meps_language);
    entries.sort_by_key(|e| {
        (
            Some(&e.dir_name) != current.map(|c| &c.dir_name),
            Some(e.meps_language) != lang,
        )
    });
    for e in entries {
        if let Some(id) = Publication::open(library, &e)?.document_by_meps_id(meps_document_id)? {
            return Ok(Some((e, id)));
        }
    }
    Ok(None)
}

/// The Bible book number if `document_id` is a book document of `entry`.
fn bible_book_of(library: &Library, entry: &Entry, document_id: i64) -> Result<Option<i64>> {
    let publication = Publication::open(library, entry)?;
    if !publication.is_bible() {
        return Ok(None);
    }
    Ok(publication
        .bible_books()?
        .into_iter()
        .find(|b| b.book_document_id == Some(document_id))
        .map(|b| b.number))
}

/// Turn a clicked link into a target inside the library, if possible.
pub fn resolve(library: &Library, current: Option<&Entry>, link: &Link) -> Result<Option<Target>> {
    Ok(match link {
        Link::Bible {
            book,
            chapter,
            verse,
        } => {
            let bible = match current {
                Some(c) if Publication::open(library, c)?.is_bible() => Some(c.clone()),
                _ => find_bible(library, current.map(|c| c.meps_language))?,
            };
            bible.map(|b| Target {
                publication: b.dir_name,
                kind: TargetKind::Chapter {
                    book: *book,
                    chapter: *chapter,
                    verse: *verse,
                },
            })
        }
        Link::Document {
            meps_document_id, ..
        } => match find_document(library, *meps_document_id, current)? {
            // A Bible book document has no text of its own: open chapter 1.
            Some((e, id)) => Some(match bible_book_of(library, &e, id)? {
                Some(book) => Target {
                    publication: e.dir_name,
                    kind: TargetKind::Chapter {
                        book,
                        chapter: 1,
                        verse: 1,
                    },
                },
                None => Target {
                    publication: e.dir_name,
                    kind: TargetKind::Document(id),
                },
            }),
            None => None,
        },
        Link::StudyNote {
            book_document,
            chapter,
            verse,
            ..
        } => match find_document(library, *book_document, current)? {
            Some((e, id)) => bible_book_of(library, &e, id)?.map(|book| Target {
                publication: e.dir_name,
                kind: TargetKind::Chapter {
                    book,
                    chapter: *chapter,
                    verse: *verse,
                },
            }),
            None => None,
        },
        Link::External(_) | Link::Fragment(_) => None,
    })
}

/// Render a target as a standalone page with images from the library.
pub fn page(library: &Library, target: &Target) -> Result<Page> {
    render(library, target, true, |_, p| {
        Some(file_url_for_dir(p.media_dir()))
    })
}

/// Render a target. `media_base` maps the publication to the URL prefix its
/// images are served under (see [`RenderOptions::media_base`]).
pub fn render(
    library: &Library,
    target: &Target,
    standalone: bool,
    media_base: impl Fn(&Entry, &Publication) -> Option<String>,
) -> Result<Page> {
    let entry = library
        .get_by_dir(&target.publication)?
        .ok_or_else(|| crate::Error::NotFound(target.publication.clone()))?;
    let publication = Publication::open(library, &entry)?;
    let renderer = Renderer::new(
        &publication,
        RenderOptions {
            media_base: media_base(&entry, &publication),
            standalone,
        },
    );
    Ok(match target.kind {
        TargetKind::Document(id) => Page {
            title: publication
                .documents()?
                .into_iter()
                .find(|d| d.id == id)
                .map(|d| d.title)
                .unwrap_or_default(),
            html: renderer.document(id)?,
            fragment: None,
            name: format!("{}-d{id}", entry.dir_name),
        },
        TargetKind::Chapter {
            book,
            chapter,
            verse,
        } => Page {
            title: format!("{} {chapter}", publication.bible_book(book)?.chapter_title),
            html: renderer.chapter(book, chapter)?,
            fragment: (verse > 1).then(|| format!("v{book}-{chapter}-{verse}-1")),
            name: format!("{}-b{book}-{chapter}", entry.dir_name),
        },
    })
}
