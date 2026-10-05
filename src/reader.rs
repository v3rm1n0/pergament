//! Read access to one imported publication: documents, Bible structure and
//! the decoded content behind them. Table semantics are in docs/FORMAT.md.

use std::path::{Path, PathBuf};

use rusqlite::{Connection, OptionalExtension, params};

use crate::crypto::ContentKey;
use crate::library::{Entry, Library};
use crate::{Error, Result, jwpub};

#[derive(Debug)]
pub struct Publication {
    entry: Entry,
    dir: PathBuf,
    conn: Connection,
    key: ContentKey,
    is_bible: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct DocumentInfo {
    pub id: i64,
    pub meps_document_id: i64,
    pub class: String,
    pub title: String,
    pub has_content: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct BibleBook {
    pub number: i64,
    pub title: String,
    /// Short-ish name used in front of chapter numbers, e.g. "Psalm".
    pub chapter_title: String,
    pub book_document_id: Option<i64>,
    pub chapters: i64,
}

/// A verse position: book, chapter, verse.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub struct VerseRef {
    pub book: i64,
    pub chapter: i64,
    pub verse: i64,
}

/// Decoded parts of one Bible chapter.
#[derive(Debug, Clone)]
pub struct Chapter {
    pub book: BibleBook,
    pub number: i64,
    pub pre_content: Option<String>,
    pub content: String,
    pub post_content: Option<String>,
    pub first_verse_id: i64,
    pub last_verse_id: i64,
}

/// (item id, parent id, title, document id, Bible book) of a navigation item.
type TocRow = (i64, i64, String, Option<i64>, Option<i64>);

/// A node of the navigation tree.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct TocNode {
    pub title: String,
    pub document_id: Option<i64>,
    /// Bible book number when the node is a Bible book.
    pub bible_book: Option<i64>,
    pub children: Vec<TocNode>,
}

#[derive(Debug, Clone)]
pub struct StudyNote {
    pub verse_id: i64,
    pub label: String,
    pub content: String,
}

impl Publication {
    pub fn open(library: &Library, entry: &Entry) -> Result<Self> {
        let conn = jwpub::open_db(&library.db_path(entry))?;
        let key = ContentKey::new(
            entry.meps_language,
            &entry.symbol,
            entry.year,
            &entry.issue_tag,
        );
        let is_bible = conn
            .query_row(
                "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'BibleChapter'",
                [],
                |_| Ok(()),
            )
            .optional()?
            .is_some();
        Ok(Self {
            entry: entry.clone(),
            dir: library.publication_dir(entry),
            conn,
            key,
            is_bible,
        })
    }

    pub fn entry(&self) -> &Entry {
        &self.entry
    }

    /// Directory holding the unpacked images.
    pub fn media_dir(&self) -> &Path {
        &self.dir
    }

    pub fn is_bible(&self) -> bool {
        self.is_bible
    }

    pub fn decode(&self, blob: &[u8]) -> Result<String> {
        self.key.decode(blob)
    }

    pub fn documents(&self) -> Result<Vec<DocumentInfo>> {
        let mut stmt = self.conn.prepare(
            "SELECT DocumentId, IFNULL(MepsDocumentId, 0), IFNULL(Class, ''), IFNULL(Title, ''),
                    Content IS NOT NULL
             FROM Document ORDER BY DocumentId",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(DocumentInfo {
                id: r.get(0)?,
                meps_document_id: r.get(1)?,
                class: r.get(2)?,
                title: r.get(3)?,
                has_content: r.get(4)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    pub fn document(&self, id: i64) -> Result<(DocumentInfo, Option<String>)> {
        let row = self
            .conn
            .query_row(
                "SELECT DocumentId, IFNULL(MepsDocumentId, 0), IFNULL(Class, ''), IFNULL(Title, ''),
                        Content
                 FROM Document WHERE DocumentId = ?1",
                [id],
                |r| {
                    let blob: Option<Vec<u8>> = r.get(4)?;
                    Ok((
                        DocumentInfo {
                            id: r.get(0)?,
                            meps_document_id: r.get(1)?,
                            class: r.get(2)?,
                            title: r.get(3)?,
                            has_content: blob.is_some(),
                        },
                        blob,
                    ))
                },
            )
            .optional()?
            .ok_or_else(|| Error::NotFound(format!("document {id}")))?;
        let html = row.1.map(|b| self.decode(&b)).transpose()?;
        Ok((row.0, html))
    }

    /// Look up a document by its MEPS document id (used by `jwpub://p/` links).
    pub fn document_by_meps_id(&self, meps_id: i64) -> Result<Option<i64>> {
        Ok(self
            .conn
            .query_row(
                "SELECT DocumentId FROM Document WHERE MepsDocumentId = ?1",
                [meps_id],
                |r| r.get(0),
            )
            .optional()?)
    }

    fn require_bible(&self) -> Result<()> {
        if self.is_bible {
            Ok(())
        } else {
            Err(Error::NotFound(format!(
                "{} is not a Bible publication",
                self.entry.symbol
            )))
        }
    }

    pub fn bible_books(&self) -> Result<Vec<BibleBook>> {
        self.require_bible()?;
        let mut stmt = self
            .conn
            .prepare(&format!("{BOOK_SELECT} ORDER BY b.BibleBookId"))?;
        let rows = stmt.query_map([], book_from_row)?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    pub fn bible_book(&self, number: i64) -> Result<BibleBook> {
        self.require_bible()?;
        self.conn
            .query_row(
                &format!("{BOOK_SELECT} WHERE b.BibleBookId = ?1"),
                [number],
                book_from_row,
            )
            .optional()?
            .ok_or_else(|| Error::NotFound(format!("Bible book {number}")))
    }

    pub fn chapter(&self, book: i64, chapter: i64) -> Result<Chapter> {
        let book = self.bible_book(book)?;
        let row = self
            .conn
            .query_row(
                "SELECT Content, PreContent, PostContent, FirstVerseId, LastVerseId
                 FROM BibleChapter WHERE BookNumber = ?1 AND ChapterNumber = ?2",
                params![book.number, chapter],
                |r| {
                    Ok((
                        r.get::<_, Vec<u8>>(0)?,
                        r.get::<_, Option<Vec<u8>>>(1)?,
                        r.get::<_, Option<Vec<u8>>>(2)?,
                        r.get::<_, i64>(3)?,
                        r.get::<_, i64>(4)?,
                    ))
                },
            )
            .optional()?
            .ok_or_else(|| Error::NotFound(format!("{} {chapter}", book.chapter_title)))?;
        Ok(Chapter {
            number: chapter,
            content: self.decode(&row.0)?,
            pre_content: row.1.map(|b| self.decode(&b)).transpose()?,
            post_content: row.2.map(|b| self.decode(&b)).transpose()?,
            first_verse_id: row.3,
            last_verse_id: row.4,
            book,
        })
    }

    /// Footnote HTML by `FootnoteIndex` within a document.
    pub fn footnote(&self, document_id: i64, index: i64) -> Result<Option<String>> {
        let blob: Option<Vec<u8>> = self
            .conn
            .query_row(
                "SELECT Content FROM Footnote WHERE DocumentId = ?1 AND FootnoteIndex = ?2",
                params![document_id, index],
                |r| r.get(0),
            )
            .optional()?
            .flatten();
        blob.map(|b| self.decode(&b)).transpose()
    }

    /// Cross references of one marginal block: `(first, last)` verse ids.
    pub fn citations(&self, document_id: i64, block: i64) -> Result<Vec<(i64, i64)>> {
        let mut stmt = self.conn.prepare(
            "SELECT FirstBibleVerseId, IFNULL(LastBibleVerseId, FirstBibleVerseId)
             FROM BibleCitation WHERE DocumentId = ?1 AND BlockNumber = ?2
             ORDER BY ElementNumber",
        )?;
        let rows = stmt.query_map(params![document_id, block], |r| Ok((r.get(0)?, r.get(1)?)))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Map a `BibleVerseId` to book/chapter/verse. The verse number comes
    /// from `BibleVerse.Label`; the first verse of a chapter is labelled
    /// with the chapter number (class `cl`) instead, and Psalm
    /// superscriptions (verse 0) have an empty label.
    pub fn verse_ref(&self, verse_id: i64) -> Result<VerseRef> {
        self.require_bible()?;
        let (book, chapter, label): (i64, i64, String) = self
            .conn
            .query_row(
                "SELECT c.BookNumber, c.ChapterNumber, IFNULL(v.Label, '')
                 FROM BibleVerse v JOIN BibleChapter c
                   ON v.BibleVerseId BETWEEN c.FirstVerseId AND c.LastVerseId
                 WHERE v.BibleVerseId = ?1",
                [verse_id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()?
            .ok_or_else(|| Error::NotFound(format!("verse id {verse_id}")))?;
        let verse = if label.trim().is_empty() {
            0
        } else if label.contains("class=\"cl\"") {
            1
        } else {
            label
                .chars()
                .filter(char::is_ascii_digit)
                .collect::<String>()
                .parse()
                .unwrap_or(1)
        };
        Ok(VerseRef {
            book,
            chapter,
            verse,
        })
    }

    /// The publication's navigation tree. Uses the `jwpub` view when there
    /// are several (the Bible's has the tabs EINFÜHRUNG, BÜCHER, INDEX, …).
    pub fn toc(&self) -> Result<Vec<TocNode>> {
        let has_views: Option<i64> = self
            .conn
            .query_row(
                "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'PublicationViewItem'",
                [],
                |r| r.get(0),
            )
            .optional()?;
        if has_views.is_none() {
            return Ok(Vec::new());
        }
        let view: Option<i64> = self
            .conn
            .query_row(
                "SELECT PublicationViewId FROM PublicationView
                 ORDER BY Symbol = 'jwpub' DESC, PublicationViewId LIMIT 1",
                [],
                |r| r.get(0),
            )
            .optional()?;
        let Some(view) = view else {
            return Ok(Vec::new());
        };
        // Only Bibles have a BibleBook table to map book documents to numbers.
        let sql = if self.is_bible {
            "SELECT i.PublicationViewItemId, i.ParentPublicationViewItemId, IFNULL(i.Title, ''),
                    i.DefaultDocumentId, b.BibleBookId
             FROM PublicationViewItem i
             LEFT JOIN BibleBook b ON b.BookDocumentId = i.DefaultDocumentId
             WHERE i.PublicationViewId = ?1
             ORDER BY i.PublicationViewItemId"
        } else {
            "SELECT PublicationViewItemId, ParentPublicationViewItemId, IFNULL(Title, ''),
                    DefaultDocumentId, NULL
             FROM PublicationViewItem
             WHERE PublicationViewId = ?1
             ORDER BY PublicationViewItemId"
        };
        let mut stmt = self.conn.prepare(sql)?;
        let rows: Vec<TocRow> = stmt
            .query_map([view], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
            })?
            .collect::<rusqlite::Result<_>>()?;

        fn build(parent: i64, rows: &[TocRow], depth: usize) -> Vec<TocNode> {
            if depth > 16 {
                return Vec::new(); // malformed or cyclic tree
            }
            rows.iter()
                .filter(|r| r.1 == parent)
                .map(|r| TocNode {
                    title: r.2.clone(),
                    document_id: r.3.filter(|d| *d >= 0),
                    bible_book: r.4,
                    children: build(r.0, rows, depth + 1),
                })
                .collect()
        }
        Ok(build(-1, &rows, 0))
    }

    /// File name of the cover image inside the publication directory.
    pub fn cover_image(&self) -> Result<Option<String>> {
        let mut stmt = self.conn.prepare(
            "SELECT FilePath FROM Multimedia
             WHERE FilePath LIKE '%\\_cvr.jpg' ESCAPE '\\' OR FilePath LIKE '%\\_sqr%' ESCAPE '\\'
             ORDER BY FilePath LIKE '%\\_cvr.jpg' ESCAPE '\\' DESC, Width DESC",
        )?;
        let names: Vec<String> = stmt
            .query_map([], |r| r.get(0))?
            .collect::<rusqlite::Result<_>>()?;
        Ok(names.into_iter().find(|n| {
            crate::render::media_name(&format!("jwpub-media://{n}")).is_some()
                && self.dir.join(n).is_file()
        }))
    }

    /// Study notes attached to verses in `first..=last`.
    pub fn study_notes(&self, first: i64, last: i64) -> Result<Vec<StudyNote>> {
        if !self.is_bible {
            return Ok(Vec::new());
        }
        let mut stmt = self.conn.prepare(
            "SELECT m.BibleVerseId, IFNULL(c.Label, ''), c.Content
             FROM VerseCommentaryMap m JOIN VerseCommentary c USING (VerseCommentaryId)
             WHERE m.BibleVerseId BETWEEN ?1 AND ?2 AND c.Content IS NOT NULL
             ORDER BY m.BibleVerseId, c.VerseCommentaryId",
        )?;
        let rows = stmt.query_map(params![first, last], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, Vec<u8>>(2)?,
            ))
        })?;
        let mut notes = Vec::new();
        for row in rows {
            let (verse_id, label, blob) = row?;
            notes.push(StudyNote {
                verse_id,
                label,
                content: self.decode(&blob)?,
            });
        }
        Ok(notes)
    }
}

const BOOK_SELECT: &str = "SELECT b.BibleBookId, IFNULL(b.BookDisplayTitle, ''),
        IFNULL(b.ChapterDisplayTitle, ''), b.BookDocumentId,
        (SELECT count(*) FROM BibleChapter c WHERE c.BookNumber = b.BibleBookId)
     FROM BibleBook b";

fn book_from_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<BibleBook> {
    Ok(BibleBook {
        number: r.get(0)?,
        title: r.get(1)?,
        chapter_title: r.get(2)?,
        book_document_id: r.get(3)?,
        chapters: r.get(4)?,
    })
}
