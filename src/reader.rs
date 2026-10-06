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

/// One `DatedText` row with its decoded content.
#[derive(Debug, Clone)]
pub struct DatedText {
    /// Document the entry belongs to (the month of a daily text, the week of a
    /// workbook, the table of contents of a study edition).
    pub document_id: i64,
    /// First and last date covered, `YYYYMMDD`.
    pub first: i64,
    pub last: i64,
    pub content: String,
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

    /// The symbol user data refers to: the undated one for issues (`w` for
    /// `w26` 2026-08), the symbol itself otherwise (`es26`, `nwtsty`).
    pub fn key_symbol(&self) -> Result<String> {
        Ok(self
            .conn
            .query_row(
                "SELECT CASE WHEN IssueTagNumber > 0 THEN UndatedSymbol ELSE Symbol END
                 FROM Publication LIMIT 1",
                [],
                |r| r.get::<_, Option<String>>(0),
            )
            .optional()?
            .flatten()
            .unwrap_or_else(|| self.entry.symbol.clone()))
    }

    /// The `DatedText` entry covering `date` (`YYYYMMDD`): a day of the daily
    /// text, or a week of a workbook or study edition.
    pub fn dated_text(&self, date: i64) -> Result<Option<DatedText>> {
        if !self.has_table("DatedText")? {
            return Ok(None);
        }
        let row = self
            .conn
            .query_row(
                "SELECT DocumentId, FirstDateOffset, LastDateOffset, Content FROM DatedText
                 WHERE ?1 BETWEEN FirstDateOffset AND LastDateOffset AND Content IS NOT NULL
                 ORDER BY FirstDateOffset DESC LIMIT 1",
                [date],
                |r| {
                    Ok((
                        r.get::<_, i64>(0)?,
                        r.get::<_, i64>(1)?,
                        r.get::<_, i64>(2)?,
                        r.get::<_, Vec<u8>>(3)?,
                    ))
                },
            )
            .optional()?;
        row.map(|(document_id, first, last, blob)| {
            Ok(DatedText {
                document_id,
                first,
                last,
                content: self.decode(&blob)?,
            })
        })
        .transpose()
    }

    fn has_table(&self, name: &str) -> Result<bool> {
        Ok(self
            .conn
            .query_row(
                "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1",
                [name],
                |_| Ok(()),
            )
            .optional()?
            .is_some())
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

    /// Footnotes of a Bible book document attached to verses in
    /// `first..=last`: `(verse id, footnote index, html)`.
    pub fn footnotes_for_verses(
        &self,
        document_id: i64,
        first: i64,
        last: i64,
    ) -> Result<Vec<(i64, i64, String)>> {
        let mut stmt = self.conn.prepare(
            "SELECT BibleVerseId, FootnoteIndex, Content FROM Footnote
             WHERE DocumentId = ?1 AND BibleVerseId BETWEEN ?2 AND ?3 AND Content IS NOT NULL
             ORDER BY BibleVerseId, FootnoteIndex",
        )?;
        let rows = stmt.query_map(params![document_id, first, last], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, Vec<u8>>(2)?,
            ))
        })?;
        let mut out = Vec::new();
        for row in rows {
            let (verse, index, blob) = row?;
            out.push((verse, index, self.decode(&blob)?));
        }
        Ok(out)
    }

    /// Cross references of a Bible book document attached to verses in
    /// `first..=last`: `(verse id, block, first verse, last verse)`, in order.
    pub fn citations_for_verses(
        &self,
        document_id: i64,
        first: i64,
        last: i64,
    ) -> Result<Vec<(i64, i64, i64, i64)>> {
        let mut stmt = self.conn.prepare(
            "SELECT BibleVerseId, BlockNumber, FirstBibleVerseId,
                    IFNULL(LastBibleVerseId, FirstBibleVerseId)
             FROM BibleCitation
             WHERE DocumentId = ?1 AND BibleVerseId BETWEEN ?2 AND ?3
             ORDER BY BibleVerseId, BlockNumber, ElementNumber",
        )?;
        let rows = stmt.query_map(params![document_id, first, last], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Title of the book's outline document, e.g. "1. Mose: Übersicht".
    pub fn overview_title(&self, book: i64) -> Result<Option<String>> {
        self.require_bible()?;
        Ok(self
            .conn
            .query_row(
                "SELECT d.Title FROM BibleBook b
                 JOIN Document d ON d.DocumentId = COALESCE(b.OutlineDocumentId, b.OverviewDocumentId)
                 WHERE b.BibleBookId = ?1",
                [book],
                |r| r.get(0),
            )
            .optional()?)
    }

    /// Outline entries (below the chapter level) overlapping `chapter`.
    /// Uses the overview outline (class 115) where a book has one.
    pub fn outline(&self, book: i64, chapter: i64) -> Result<Vec<OutlineEntry>> {
        self.require_bible()?;
        let class: Option<i64> = self
            .conn
            .query_row(
                "SELECT Class FROM BibleOutlineEntry WHERE Book = ?1
                 GROUP BY Class ORDER BY Class = 115 DESC, Class LIMIT 1",
                [book],
                |r| r.get(0),
            )
            .optional()?;
        let Some(class) = class else {
            return Ok(Vec::new());
        };
        let mut stmt = self.conn.prepare(
            "SELECT Level, BeginChapterNumber, IFNULL(BeginVerseNumber, 1),
                    EndChapterNumber, EndVerseNumber, Content
             FROM BibleOutlineEntry
             WHERE Book = ?1 AND Class = ?2 AND Level > 1
               AND BeginChapterNumber <= ?3 AND EndChapterNumber >= ?3 AND Content IS NOT NULL
             ORDER BY BibleOutlineEntryId",
        )?;
        let rows = stmt.query_map(params![book, class, chapter], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, i64>(3)?,
                r.get::<_, Option<i64>>(4)?,
                r.get::<_, Vec<u8>>(5)?,
            ))
        })?;
        let mut out = Vec::new();
        for row in rows {
            let (level, bc, bv, ec, ev, blob) = row?;
            let text = outline_text(&self.decode(&blob)?);
            if text.is_empty() {
                continue;
            }
            out.push(OutlineEntry {
                level,
                text,
                begin_chapter: bc,
                begin_verse: bv,
                end_chapter: ec,
                end_verse: ev,
            });
        }
        Ok(out)
    }

    /// Abbreviated Bible book names (e.g. 1 → "1Mo", 44 → "Apg"), mined from
    /// the labels of Bible links in the publication itself; the most common
    /// abbreviation per book wins. Cached next to the publication files.
    pub fn book_abbreviations(&self) -> Result<std::collections::HashMap<i64, String>> {
        use std::collections::HashMap;
        let cache = self.dir.join(".jwlinux-abbreviations.json");
        if let Ok(bytes) = std::fs::read(&cache)
            && let Ok(map) = serde_json::from_slice::<HashMap<i64, String>>(&bytes)
        {
            return Ok(map);
        }
        let mut votes: HashMap<i64, HashMap<String, u32>> = HashMap::new();
        for sql in [
            "SELECT Content FROM VerseCommentary WHERE Content IS NOT NULL",
            "SELECT Content FROM Footnote WHERE Content IS NOT NULL",
            "SELECT Content FROM Document WHERE Content IS NOT NULL",
        ] {
            let mut stmt = match self.conn.prepare(sql) {
                Ok(s) => s,
                Err(_) => continue, // table missing in this publication
            };
            let blobs = stmt.query_map([], |r| r.get::<_, Vec<u8>>(0))?;
            for blob in blobs {
                let Ok(html) = self.decode(&blob?) else {
                    continue;
                };
                for (book, abbr) in bible_link_labels(&html) {
                    *votes.entry(book).or_default().entry(abbr).or_default() += 1;
                }
            }
        }
        let map: HashMap<i64, String> = votes
            .into_iter()
            .filter_map(|(book, v)| {
                v.into_iter()
                    .max_by(|a, b| a.1.cmp(&b.1).then_with(|| b.0.len().cmp(&a.0.len())))
                    .map(|(abbr, _)| (book, abbr))
            })
            .collect();
        let _ = std::fs::write(&cache, serde_json::to_vec(&map)?);
        Ok(map)
    }
}

/// An outline line, e.g. "Abram zieht von Haran nach Kanaan" for 12:1-9.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct OutlineEntry {
    pub level: i64,
    pub text: String,
    pub begin_chapter: i64,
    pub begin_verse: i64,
    pub end_chapter: i64,
    pub end_verse: Option<i64>,
}

/// Text of the innermost `<p>` of an outline entry, without the trailing
/// verse range in parentheses.
fn outline_text(html: &str) -> String {
    let last_p = html.rfind("<p").map_or(html, |i| &html[i..]);
    let p = last_p.split("</p>").next().unwrap_or(last_p);
    let text = strip_tags(p);
    let text = match text.rfind('(') {
        Some(i) if text.trim_end().ends_with(')') => &text[..i],
        _ => &text,
    };
    text.trim().to_owned()
}

/// Remove tags and decode the few entities that occur in publication text.
pub(crate) fn strip_tags(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut in_tag = false;
    for c in html.chars() {
        match c {
            '<' => in_tag = true,
            '>' if in_tag => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    out.replace("&nbsp;", "\u{a0}")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
}

/// `(book, abbreviation)` for each `<a href="jwpub://b/NWTR/B:…">Abbr C:V</a>`
/// whose label starts with a book abbreviation followed by a chapter.
fn bible_link_labels(html: &str) -> Vec<(i64, String)> {
    const HREF: &str = "href=\"jwpub://b/";
    let mut out = Vec::new();
    let mut rest = html;
    while let Some(i) = rest.find(HREF) {
        rest = &rest[i + HREF.len()..];
        // `NWTR/40:1:1-40:1:1"…>label<`
        let Some(slash) = rest.find('/') else { break };
        let book: Option<i64> = rest[slash + 1..]
            .split(':')
            .next()
            .and_then(|b| b.parse().ok());
        let (Some(book), Some(gt)) = (book, rest.find('>')) else {
            continue;
        };
        let label_start = &rest[gt + 1..];
        let label = label_start
            .split('<')
            .next()
            .unwrap_or_default()
            .trim_start();
        if let Some(abbr) = abbreviation_of(label) {
            out.push((book, abbr));
        }
    }
    out
}

/// "1Mo 5:1;" -> "1Mo", "Apg 7:3" -> "Apg"; `None` if the label does not
/// start with a word followed by a chapter number.
fn abbreviation_of(label: &str) -> Option<String> {
    let (word, tail) = label.split_once(' ')?;
    let word = word.trim_end_matches('.');
    let mut chars = word.chars();
    let first = chars.next()?;
    let rest: String = chars.collect();
    let valid = (first.is_ascii_digit() || first.is_alphabetic())
        && !rest.is_empty()
        && rest.chars().all(char::is_alphabetic)
        && word.chars().filter(char::is_ascii_digit).count() <= 1
        && word.chars().count() <= 6;
    (valid && tail.starts_with(|c: char| c.is_ascii_digit())).then(|| word.to_owned())
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn abbreviations_from_labels() {
        assert_eq!(abbreviation_of("1Mo 5:1;").as_deref(), Some("1Mo"));
        assert_eq!(abbreviation_of("Apg 7:3, 4").as_deref(), Some("Apg"));
        assert_eq!(abbreviation_of("Vers 3"), Some("Vers".into())); // filtered by majority vote
        assert_eq!(abbreviation_of("Offenbarung 1:1"), None); // too long to be an abbreviation
        assert_eq!(abbreviation_of(" 6:9;"), None);
        assert_eq!(abbreviation_of("12:1"), None);
        let html = r#"(<a href="jwpub://b/NWTR/1:5:1-1:5:1" data-bid="3-1" class="b">1Mo 5:1;</a><a href="jwpub://b/NWTR/1:6:9-1:6:9" class="b"> 6:9;</a>"#;
        assert_eq!(bible_link_labels(html), vec![(1, "1Mo".to_owned())]);
    }

    #[test]
    fn outline_texts() {
        let html = r#"<ul class="outline"><li class="L1 chapterNo"><ul><li class="L2"><p id="p53" data-pid="53">Abram zieht von Haran nach Kanaan <span class="altsize">(</span><a class="it" href="jwpub://c/X:1/12:1-12:9"><span class="altsize">1-9</span></a><span class="altsize">)</span></p></li></ul></li></ul>"#;
        assert_eq!(outline_text(html), "Abram zieht von Haran nach Kanaan");
        assert_eq!(strip_tags("a&nbsp;<b>b</b> &amp; c"), "a\u{a0}b & c");
    }
}
