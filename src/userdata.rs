//! Personal study data — highlights, notes, tags and bookmarks — stored in a
//! database with the layout of a JW Library `userData.db`, and `.jwlibrary`
//! backups of it. Table semantics are in docs/FORMAT.md ("User data").

use std::collections::{BTreeMap, HashMap};
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use rusqlite::{Connection, OpenFlags, OptionalExtension, Transaction, params};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use zip::ZipArchive;
use zip::write::SimpleFileOptions;

use crate::sync_snapshot::{
    DeleteGuard, Plan, Resolver, RowKey, RowRef, Side, Snapshot, SyncMerge, SyncOutcome, classify,
    rows_of, rows_of_file,
};
use crate::{Error, Result};

/// File name of the database, inside the library root and inside backups.
pub const DB_NAME: &str = "userData.db";
/// Schema version of a database created here (kept in `PRAGMA user_version`).
pub const SCHEMA_VERSION: i64 = 16;
/// Device name written into backups.
pub const DEVICE_NAME: &str = "Pergament";

/// Tables whose rows make up the content fingerprint.
const FINGERPRINT_TABLES: [&str; 8] = [
    "Location",
    "UserMark",
    "BlockRange",
    "Note",
    "Tag",
    "TagMap",
    "Bookmark",
    "InputField",
];

const MANIFEST: &str = "manifest.json";
const THUMBNAIL: &str = "default_thumbnail.png";
const THUMBNAIL_PNG: &[u8] = include_bytes!("../assets/backup_thumbnail.png");
const MAX_MANIFEST: u64 = 64 * 1024;
const MAX_DATABASE: u64 = 1 << 30;
/// Tables a backup must contain to be accepted.
const REQUIRED_TABLES: [&str; 8] = [
    "Location",
    "UserMark",
    "BlockRange",
    "Note",
    "Tag",
    "TagMap",
    "Bookmark",
    "LastModified",
];

/// `BlockRange.BlockType` / `Note.BlockType`: a paragraph (`data-pid`).
pub const BLOCK_PARAGRAPH: i64 = 1;
/// A Bible verse (verse number).
pub const BLOCK_VERSE: i64 = 2;
/// `Tag.Type` of user tags (0 is the built-in favorites list).
pub(crate) const TAG_USER: i64 = 1;

/// Database identity of a whole publication, independent of its download.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PublicationKey {
    pub key_symbol: String,
    pub meps_language: i64,
    pub issue_tag: i64,
}

impl PublicationKey {
    pub fn validate(&self) -> Result<()> {
        if self.key_symbol.is_empty()
            || self
                .key_symbol
                .chars()
                .any(|c| c.is_whitespace() || c.is_control())
            || self.meps_language < 0
            || self.issue_tag < 0
        {
            return Err(Error::NotFound("invalid publication identity".into()));
        }
        Ok(())
    }
}

/// The page a mark or note belongs to: a document of a publication, or a
/// chapter of a Bible.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Loc {
    pub key_symbol: String,
    pub meps_language: i64,
    /// 0 for undated publications.
    pub issue_tag: i64,
    /// MEPS document id (publications).
    pub document_id: Option<i64>,
    /// Book and chapter (Bibles).
    pub book: Option<i64>,
    pub chapter: Option<i64>,
}

/// Highlighted tokens `start..=end` of one paragraph or verse.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Range {
    pub block_type: i64,
    pub identifier: i64,
    pub start: i64,
    pub end: i64,
}

/// A highlight. `color` is `UserMark.ColorIndex` (1-6).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Mark {
    pub guid: String,
    pub color: i64,
    pub ranges: Vec<Range>,
}

/// Where a note sits, for lists of notes across publications.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NoteLocation {
    #[serde(flatten)]
    pub loc: Loc,
    pub title: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Note {
    pub guid: String,
    pub title: String,
    pub content: String,
    /// 0 (whole page), [`BLOCK_PARAGRAPH`] or [`BLOCK_VERSE`].
    pub block_type: i64,
    pub block_identifier: Option<i64>,
    /// The highlight the note is attached to, and its color.
    pub mark_guid: Option<String>,
    pub color: Option<i64>,
    pub tags: Vec<String>,
    pub last_modified: String,
    pub location: Option<NoteLocation>,
}

/// A note to create (no `guid`) or update.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NoteInput {
    pub guid: Option<String>,
    pub title: String,
    pub content: String,
    #[serde(default)]
    pub block_type: i64,
    pub block_identifier: Option<i64>,
    pub mark_guid: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TagInfo {
    pub id: i64,
    pub name: String,
    pub notes: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Bookmark {
    pub slot: i64,
    pub title: String,
    pub snippet: Option<String>,
    pub block_type: i64,
    pub block_identifier: Option<i64>,
    pub location: NoteLocation,
}

/// Counts shown after a backup was restored.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    pub marks: i64,
    pub notes: i64,
    pub tags: i64,
    pub bookmarks: i64,
    /// Device the backup was made on.
    pub device: Option<String>,
}

/// Counts shown after a backup was merged into the current data.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MergeReport {
    /// Rows only the backup had, now added.
    pub added: i64,
    /// Rows on both sides where the backup's version replaced the current one.
    pub updated: i64,
    /// Rows on both sides that were the same already, or where the current
    /// note is newer by its own time.
    pub unchanged: i64,
    /// Backup rows that differ from the current ones and were left out
    /// because the backup was not newer. A sync merge also lists rows
    /// changed on both sides, whichever version won, and rows changed on
    /// one side and deleted on the other, where the changed row is kept.
    pub conflicts: Vec<Conflict>,
    /// Device the backup was made on.
    pub device: Option<String>,
}

/// A backup row that differs from the current one and was left out, as the
/// backup was not newer; the current row is kept. In a sync merge, also a
/// row changed on both sides or changed on one and deleted on the other
/// (see [`MergeReport::conflicts`]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Conflict {
    /// Table of the row, e.g. `UserMark`.
    pub table: String,
    /// GUID of the row, or its key in tables without one.
    pub guid: String,
    /// Where the row belongs, for the user.
    pub summary: String,
}

/// The parts of a backup's `manifest.json` that matter here.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct BackupManifest {
    creation_date: String,
    version: i64,
    name: String,
    #[serde(rename = "type")]
    kind: i64,
    user_data_backup: BackupInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BackupInfo {
    last_modified_date: String,
    hash: String,
    database_name: String,
    device_name: String,
    schema_version: i64,
}

pub struct UserData {
    conn: Connection,
    path: PathBuf,
}

impl std::fmt::Debug for UserData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UserData")
            .field("path", &self.path)
            .finish()
    }
}

impl UserData {
    /// Open the database at `path`, creating an empty one if it is missing.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref().to_owned();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(&path)?;
        conn.execute_batch(
            "PRAGMA trusted_schema = OFF; PRAGMA foreign_keys = ON; PRAGMA busy_timeout = 2000;",
        )?;
        let has_schema: bool = conn.query_row(
            "SELECT count(*) > 0 FROM sqlite_master WHERE type = 'table' AND name = 'UserMark'",
            [],
            |r| r.get(0),
        )?;
        if !has_schema {
            conn.execute_batch(SCHEMA)?;
            conn.pragma_update(None, "user_version", SCHEMA_VERSION)?;
        }
        Ok(Self { conn, path })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The open database, for reading in sibling modules.
    pub(crate) fn conn(&self) -> &Connection {
        &self.conn
    }

    pub fn summary(&self) -> Result<Summary> {
        let count = |sql: &str| -> Result<i64> { Ok(self.conn.query_row(sql, [], |r| r.get(0))?) };
        Ok(Summary {
            marks: count("SELECT count(*) FROM UserMark")?,
            notes: count("SELECT count(*) FROM Note")?,
            tags: count(&format!("SELECT count(*) FROM Tag WHERE Type = {TAG_USER}"))?,
            bookmarks: count("SELECT count(*) FROM Bookmark")?,
            device: None,
        })
    }

    /// Whole-publication favorites in their saved order, in every language.
    pub fn favorites(&self) -> Result<Vec<PublicationKey>> {
        let mut stmt = self.conn.prepare(
            "SELECT l.KeySymbol, l.MepsLanguage, l.IssueTagNumber
             FROM TagMap m JOIN Tag t ON t.TagId = m.TagId
             JOIN Location l ON l.LocationId = m.LocationId
             WHERE t.Type = 0 AND l.Type = 1 AND l.BookNumber IS NULL
               AND l.ChapterNumber IS NULL AND l.DocumentId IS NULL AND l.Track IS NULL
             ORDER BY m.Position, m.TagMapId",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(PublicationKey {
                key_symbol: r.get(0)?,
                meps_language: r.get(1)?,
                issue_tag: r.get(2)?,
            })
        })?;
        let mut favorites = Vec::new();
        for row in rows {
            let key = row?;
            if !favorites.contains(&key) {
                favorites.push(key);
            }
        }
        Ok(favorites)
    }

    /// Set membership, not toggle: repeated requests leave the saved order alone.
    pub fn set_favorite(&mut self, key: &PublicationKey, favorite: bool) -> Result<()> {
        key.validate()?;
        let tx = self.conn.transaction()?;
        let locations = "SELECT LocationId FROM Location
            WHERE Type = 1 AND KeySymbol = ?1 AND MepsLanguage = ?2 AND IssueTagNumber = ?3
              AND BookNumber IS NULL AND ChapterNumber IS NULL
              AND DocumentId IS NULL AND Track IS NULL";
        let args = params![key.key_symbol, key.meps_language, key.issue_tag];
        let changed = if favorite {
            let exists: bool = tx.query_row(
                &format!(
                    "SELECT EXISTS(SELECT 1 FROM TagMap m JOIN Tag t ON t.TagId = m.TagId
                         WHERE t.Type = 0 AND m.LocationId IN ({locations}))"
                ),
                args,
                |r| r.get(0),
            )?;
            if exists {
                return Ok(());
            }
            let location: Option<i64> = tx
                .query_row(
                    &format!("{locations} ORDER BY LocationId LIMIT 1"),
                    args,
                    |r| r.get(0),
                )
                .optional()?;
            let location = match location {
                Some(id) => id,
                None => {
                    tx.execute(
                        "INSERT INTO Location (KeySymbol, MepsLanguage, IssueTagNumber, Type)
                         VALUES (?1, ?2, ?3, 1)",
                        args,
                    )?;
                    tx.last_insert_rowid()
                }
            };
            tx.execute(
                "INSERT INTO Tag (Type, Name) SELECT 0, 'Favorite'
                 WHERE NOT EXISTS (SELECT 1 FROM Tag WHERE Type = 0)",
                [],
            )?;
            tx.execute(
                "INSERT INTO TagMap (LocationId, TagId, Position)
                 SELECT ?1, TagId, (SELECT IFNULL(max(Position), -1) + 1
                                   FROM TagMap WHERE TagId = t.TagId)
                 FROM Tag t WHERE Type = 0 ORDER BY TagId LIMIT 1",
                [location],
            )?
        } else {
            tx.execute(
                &format!(
                    "DELETE FROM TagMap WHERE TagId IN (SELECT TagId FROM Tag WHERE Type = 0)
                          AND LocationId IN ({locations})"
                ),
                args,
            )?
        };
        if changed > 0 {
            touch(&tx)?;
        }
        Ok(tx.commit()?)
    }

    /// Location rows of a page (there can be several for the same page).
    fn location_ids(&self, loc: &Loc) -> Result<Vec<i64>> {
        let mut stmt = self.conn.prepare(
            "SELECT LocationId FROM Location
             WHERE Type = 0 AND Track IS NULL AND KeySymbol = ?1 AND MepsLanguage = ?2
               AND IssueTagNumber = ?3 AND IFNULL(DocumentId, 0) = IFNULL(?4, 0)
               AND IFNULL(BookNumber, 0) = IFNULL(?5, 0)
               AND IFNULL(ChapterNumber, 0) = IFNULL(?6, 0)
             ORDER BY LocationId",
        )?;
        let rows = stmt.query_map(
            params![
                loc.key_symbol,
                loc.meps_language,
                loc.issue_tag,
                loc.document_id,
                loc.book,
                loc.chapter
            ],
            |r| r.get(0),
        )?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    fn location_id(tx: &Transaction<'_>, loc: &Loc, title: Option<&str>) -> Result<i64> {
        let found: Option<i64> = tx
            .query_row(
                "SELECT LocationId FROM Location
                 WHERE Type = 0 AND Track IS NULL AND KeySymbol = ?1 AND MepsLanguage = ?2
                   AND IssueTagNumber = ?3 AND IFNULL(DocumentId, 0) = IFNULL(?4, 0)
                   AND IFNULL(BookNumber, 0) = IFNULL(?5, 0)
                   AND IFNULL(ChapterNumber, 0) = IFNULL(?6, 0)
                 ORDER BY LocationId LIMIT 1",
                params![
                    loc.key_symbol,
                    loc.meps_language,
                    loc.issue_tag,
                    loc.document_id,
                    loc.book,
                    loc.chapter
                ],
                |r| r.get(0),
            )
            .optional()?;
        if let Some(id) = found {
            if title.is_some() {
                tx.execute(
                    "UPDATE Location SET Title = ?1 WHERE LocationId = ?2 AND Title IS NULL",
                    params![title, id],
                )?;
            }
            return Ok(id);
        }
        tx.execute(
            "INSERT INTO Location (BookNumber, ChapterNumber, DocumentId, IssueTagNumber,
                                   KeySymbol, MepsLanguage, Type, Title)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, 0, ?7)",
            params![
                loc.book,
                loc.chapter,
                loc.document_id,
                loc.issue_tag,
                loc.key_symbol,
                loc.meps_language,
                title
            ],
        )?;
        Ok(tx.last_insert_rowid())
    }

    /// Location rows holding the answers of a page. JW Library leaves the
    /// language of these empty, so an empty language matches too.
    fn answer_location_ids(conn: &Connection, loc: &Loc) -> Result<Vec<i64>> {
        let mut stmt = conn.prepare(
            "SELECT LocationId FROM Location
             WHERE Type = 0 AND Track IS NULL AND KeySymbol = ?1
               AND (MepsLanguage IS NULL OR MepsLanguage = ?2)
               AND IssueTagNumber = ?3 AND IFNULL(DocumentId, 0) = IFNULL(?4, 0)
               AND IFNULL(BookNumber, 0) = IFNULL(?5, 0)
               AND IFNULL(ChapterNumber, 0) = IFNULL(?6, 0)
             ORDER BY LocationId",
        )?;
        let rows = stmt.query_map(
            params![
                loc.key_symbol,
                loc.meps_language,
                loc.issue_tag,
                loc.document_id,
                loc.book,
                loc.chapter
            ],
            |r| r.get(0),
        )?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Texts typed into the answer fields of a page, by field id (`TextTag`).
    pub fn answers(&self, loc: &Loc) -> Result<BTreeMap<String, String>> {
        let mut out = BTreeMap::new();
        for location in Self::answer_location_ids(&self.conn, loc)? {
            let mut stmt = self
                .conn
                .prepare("SELECT TextTag, Value FROM InputField WHERE LocationId = ?1")?;
            let rows = stmt.query_map([location], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
            })?;
            for row in rows {
                let (tag, value) = row?;
                out.entry(tag).or_insert(value);
            }
        }
        Ok(out)
    }

    /// Store the text of an answer field; an empty text removes it.
    pub fn save_answer(
        &mut self,
        loc: &Loc,
        title: Option<&str>,
        tag: &str,
        value: &str,
    ) -> Result<()> {
        let tx = self.conn.transaction()?;
        let locations = Self::answer_location_ids(&tx, loc)?;
        if value.is_empty() {
            for location in locations {
                tx.execute(
                    "DELETE FROM InputField WHERE LocationId = ?1 AND TextTag = ?2",
                    params![location, tag],
                )?;
            }
        } else {
            let location = match locations.first() {
                Some(id) => *id,
                None => Self::location_id(&tx, loc, title)?,
            };
            tx.execute(
                "INSERT INTO InputField (LocationId, TextTag, Value) VALUES (?1, ?2, ?3)
                 ON CONFLICT (LocationId, TextTag) DO UPDATE SET Value = excluded.Value",
                params![location, tag, value],
            )?;
        }
        touch(&tx)?;
        Ok(tx.commit()?)
    }

    /// Highlights on a page.
    pub fn marks(&self, loc: &Loc) -> Result<Vec<Mark>> {
        let mut out: Vec<Mark> = Vec::new();
        for location in self.location_ids(loc)? {
            let mut stmt = self.conn.prepare(
                "SELECT m.UserMarkGuid, m.ColorIndex, r.BlockType, r.Identifier,
                        IFNULL(r.StartToken, 0), IFNULL(r.EndToken, 0)
                 FROM UserMark m JOIN BlockRange r ON r.UserMarkId = m.UserMarkId
                 WHERE m.LocationId = ?1 ORDER BY m.UserMarkId, r.BlockRangeId",
            )?;
            let rows = stmt.query_map([location], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, i64>(1)?,
                    Range {
                        block_type: r.get(2)?,
                        identifier: r.get(3)?,
                        start: r.get(4)?,
                        end: r.get(5)?,
                    },
                ))
            })?;
            for row in rows {
                let (guid, color, range) = row?;
                match out.last_mut() {
                    Some(m) if m.guid == guid => m.ranges.push(range),
                    _ => out.push(Mark {
                        guid,
                        color,
                        ranges: vec![range],
                    }),
                }
            }
        }
        Ok(out)
    }

    /// Highlight `ranges` on a page in `color`. Overlapped parts of existing
    /// highlights are cut away, so a token has at most one color.
    pub fn add_mark(
        &mut self,
        loc: &Loc,
        title: Option<&str>,
        color: i64,
        ranges: &[Range],
    ) -> Result<String> {
        let tx = self.conn.transaction()?;
        let location = Self::location_id(&tx, loc, title)?;
        for r in ranges {
            cut_ranges(&tx, location, r)?;
        }
        let guid = new_guid(&tx)?;
        tx.execute(
            "INSERT INTO UserMark (ColorIndex, LocationId, StyleIndex, UserMarkGuid, Version)
             VALUES (?1, ?2, 0, ?3, 1)",
            params![color, location, guid],
        )?;
        let mark = tx.last_insert_rowid();
        for r in ranges {
            tx.execute(
                "INSERT INTO BlockRange (BlockType, Identifier, StartToken, EndToken, UserMarkId)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![r.block_type, r.identifier, r.start, r.end, mark],
            )?;
        }
        touch(&tx)?;
        tx.commit()?;
        Ok(guid)
    }

    pub fn set_mark_color(&mut self, guid: &str, color: i64) -> Result<()> {
        let tx = self.conn.transaction()?;
        tx.execute(
            "UPDATE UserMark SET ColorIndex = ?1 WHERE UserMarkGuid = ?2",
            params![color, guid],
        )?;
        touch(&tx)?;
        Ok(tx.commit()?)
    }

    /// Remove a highlight. Notes attached to it stay, without the highlight.
    pub fn delete_mark(&mut self, guid: &str) -> Result<()> {
        let tx = self.conn.transaction()?;
        let id: Option<i64> = tx
            .query_row(
                "SELECT UserMarkId FROM UserMark WHERE UserMarkGuid = ?1",
                [guid],
                |r| r.get(0),
            )
            .optional()?;
        if let Some(id) = id {
            delete_mark_row(&tx, id)?;
        }
        touch(&tx)?;
        Ok(tx.commit()?)
    }

    const NOTE_SELECT: &str = "SELECT n.NoteId, n.Guid, IFNULL(n.Title, ''), IFNULL(n.Content, ''),
            n.BlockType, n.BlockIdentifier, m.UserMarkGuid, m.ColorIndex, n.LastModified,
            l.KeySymbol, l.MepsLanguage, l.IssueTagNumber, l.DocumentId, l.BookNumber,
            l.ChapterNumber, l.Title
         FROM Note n
         LEFT JOIN UserMark m ON m.UserMarkId = n.UserMarkId
         LEFT JOIN Location l ON l.LocationId = n.LocationId";

    fn notes_where(&self, condition: &str, values: &[&dyn rusqlite::ToSql]) -> Result<Vec<Note>> {
        let mut stmt = self.conn.prepare(&format!(
            "{} {condition} ORDER BY n.LastModified DESC, n.NoteId DESC",
            Self::NOTE_SELECT
        ))?;
        let rows = stmt.query_map(values, |r| {
            let key_symbol: Option<String> = r.get(9)?;
            let meps_language: Option<i64> = r.get(10)?;
            let location = match (key_symbol, meps_language) {
                (Some(key_symbol), Some(meps_language)) => Some(NoteLocation {
                    loc: Loc {
                        key_symbol,
                        meps_language,
                        issue_tag: r.get(11)?,
                        document_id: r.get(12)?,
                        book: r.get(13)?,
                        chapter: r.get(14)?,
                    },
                    title: r.get(15)?,
                }),
                _ => None,
            };
            Ok((
                r.get::<_, i64>(0)?,
                Note {
                    guid: r.get(1)?,
                    title: r.get(2)?,
                    content: r.get(3)?,
                    block_type: r.get(4)?,
                    block_identifier: r.get(5)?,
                    mark_guid: r.get(6)?,
                    color: r.get(7)?,
                    tags: Vec::new(),
                    last_modified: r.get(8)?,
                    location,
                },
            ))
        })?;
        let mut notes = Vec::new();
        for row in rows {
            let (id, mut note) = row?;
            note.tags = self.note_tags(id)?;
            notes.push(note);
        }
        Ok(notes)
    }

    fn note_tags(&self, note: i64) -> Result<Vec<String>> {
        let mut stmt = self.conn.prepare(
            "SELECT t.Name FROM TagMap m JOIN Tag t ON t.TagId = m.TagId
             WHERE m.NoteId = ?1 ORDER BY t.Name",
        )?;
        let rows = stmt.query_map([note], |r| r.get(0))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Notes on a page.
    pub fn notes(&self, loc: &Loc) -> Result<Vec<Note>> {
        let mut out = Vec::new();
        for location in self.location_ids(loc)? {
            out.extend(self.notes_where("WHERE n.LocationId = ?1", &[&location])?);
        }
        Ok(out)
    }

    /// All notes, newest first, optionally only those with tag `tag`.
    pub fn all_notes(&self, tag: Option<i64>) -> Result<Vec<Note>> {
        match tag {
            Some(tag) => self.notes_where(
                "WHERE n.NoteId IN (SELECT NoteId FROM TagMap WHERE TagId = ?1)",
                &[&tag],
            ),
            None => self.notes_where("", &[]),
        }
    }

    /// Create or update a note; returns its guid. `loc` is needed to create.
    pub fn save_note(
        &mut self,
        loc: Option<&Loc>,
        title: Option<&str>,
        note: &NoteInput,
    ) -> Result<String> {
        let tx = self.conn.transaction()?;
        let existing: Option<(i64, String)> = match &note.guid {
            Some(guid) => tx
                .query_row(
                    "SELECT NoteId, Guid FROM Note WHERE Guid = ?1",
                    [guid],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .optional()?,
            None => None,
        };
        let (id, guid) = match existing {
            Some((id, guid)) => {
                tx.execute(
                    "UPDATE Note SET Title = ?1, Content = ?2,
                            LastModified = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
                     WHERE NoteId = ?3",
                    params![note.title, note.content, id],
                )?;
                (id, guid)
            }
            None => {
                let loc = loc.ok_or_else(|| Error::NotFound("page for the new note".into()))?;
                let location = Self::location_id(&tx, loc, title)?;
                let mark: Option<i64> = match &note.mark_guid {
                    Some(g) => tx
                        .query_row(
                            "SELECT UserMarkId FROM UserMark WHERE UserMarkGuid = ?1",
                            [g],
                            |r| r.get(0),
                        )
                        .optional()?,
                    None => None,
                };
                let (block_type, block) = match note.block_identifier {
                    Some(b)
                        if note.block_type == BLOCK_PARAGRAPH || note.block_type == BLOCK_VERSE =>
                    {
                        (note.block_type, Some(b))
                    }
                    _ => (0, None),
                };
                let guid = new_guid(&tx)?;
                tx.execute(
                    "INSERT INTO Note (Guid, UserMarkId, LocationId, Title, Content,
                                       BlockType, BlockIdentifier)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                    params![
                        guid,
                        mark,
                        location,
                        note.title,
                        note.content,
                        block_type,
                        block
                    ],
                )?;
                (tx.last_insert_rowid(), guid)
            }
        };
        set_note_tags(&tx, id, &note.tags)?;
        touch(&tx)?;
        tx.commit()?;
        Ok(guid)
    }

    pub fn delete_note(&mut self, guid: &str) -> Result<()> {
        let tx = self.conn.transaction()?;
        let id: Option<i64> = tx
            .query_row("SELECT NoteId FROM Note WHERE Guid = ?1", [guid], |r| {
                r.get(0)
            })
            .optional()?;
        if let Some(id) = id {
            delete_note_row(&tx, id)?;
        }
        touch(&tx)?;
        Ok(tx.commit()?)
    }

    /// User tags with the number of notes carrying them.
    pub fn tags(&self) -> Result<Vec<TagInfo>> {
        let mut stmt = self.conn.prepare(
            "SELECT t.TagId, t.Name, count(m.NoteId) FROM Tag t
             LEFT JOIN TagMap m ON m.TagId = t.TagId
             WHERE t.Type = ?1 GROUP BY t.TagId ORDER BY t.Name COLLATE NOCASE",
        )?;
        let rows = stmt.query_map([TAG_USER], |r| {
            Ok(TagInfo {
                id: r.get(0)?,
                name: r.get(1)?,
                notes: r.get(2)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    pub fn bookmarks(&self) -> Result<Vec<Bookmark>> {
        let mut stmt = self.conn.prepare(
            "SELECT b.Slot, b.Title, b.Snippet, b.BlockType, b.BlockIdentifier,
                    l.KeySymbol, IFNULL(l.MepsLanguage, 0), l.IssueTagNumber, l.DocumentId,
                    l.BookNumber, l.ChapterNumber, l.Title
             FROM Bookmark b JOIN Location l ON l.LocationId = b.LocationId
             WHERE l.KeySymbol IS NOT NULL ORDER BY b.Slot",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(Bookmark {
                slot: r.get(0)?,
                title: r.get(1)?,
                snippet: r.get(2)?,
                block_type: r.get(3)?,
                block_identifier: r.get(4)?,
                location: NoteLocation {
                    loc: Loc {
                        key_symbol: r.get(5)?,
                        meps_language: r.get(6)?,
                        issue_tag: r.get(7)?,
                        document_id: r.get(8)?,
                        book: r.get(9)?,
                        chapter: r.get(10)?,
                    },
                    title: r.get(11)?,
                },
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Hash of the user's content: every row of the data tables, but not the
    /// database-wide `LastModified` clock, playlists or media. A merge keeps
    /// the later clock, so only the rows tell whether anything changed.
    pub fn fingerprint(&self) -> Result<String> {
        use rusqlite::types::ValueRef;

        fn put(hasher: &mut Sha256, tag: u8, bytes: &[u8]) {
            hasher.update([tag]);
            hasher.update((bytes.len() as u64).to_le_bytes());
            hasher.update(bytes);
        }

        let mut hasher = Sha256::new();
        for table in FINGERPRINT_TABLES {
            put(&mut hasher, b'T', table.as_bytes());
            let columns = self
                .conn
                .prepare(&format!("SELECT * FROM {table}"))?
                .column_count();
            let order = (1..=columns)
                .map(|i| i.to_string())
                .collect::<Vec<_>>()
                .join(", ");
            let mut stmt = self
                .conn
                .prepare(&format!("SELECT * FROM {table} ORDER BY {order}"))?;
            let mut rows = stmt.query([])?;
            while let Some(row) = rows.next()? {
                hasher.update(*b"R");
                for i in 0..columns {
                    match row.get_ref(i)? {
                        ValueRef::Null => put(&mut hasher, b'N', &[]),
                        ValueRef::Integer(v) => put(&mut hasher, b'I', &v.to_le_bytes()),
                        ValueRef::Real(v) => put(&mut hasher, b'F', &v.to_le_bytes()),
                        ValueRef::Text(v) => put(&mut hasher, b'S', v),
                        ValueRef::Blob(v) => put(&mut hasher, b'B', v),
                    }
                }
            }
        }
        Ok(hex::encode(hasher.finalize()))
    }

    /// Local time as `YYYY-MM-DD-HHMMSS`, for file names.
    pub fn local_stamp(&self) -> Result<String> {
        Ok(self.conn.query_row(
            "SELECT strftime('%Y-%m-%d-%H%M%S', 'now', 'localtime')",
            [],
            |r| r.get(0),
        )?)
    }

    /// Write a `.jwlibrary` backup of the current data to `dest`.
    pub fn export_backup(&self, dest: &Path) -> Result<()> {
        self.export_backup_as(dest, DEVICE_NAME)
    }

    /// Like `export_backup`, with `device_name` in the manifest.
    pub fn export_backup_as(&self, dest: &Path, device_name: &str) -> Result<()> {
        let dir = tempfile::tempdir()?;
        let copy = dir.path().join(DB_NAME);
        self.conn
            .execute("VACUUM INTO ?1", [copy.to_string_lossy().as_ref()])?;
        let hash = hex::encode(Sha256::digest(fs::read(&copy)?));
        let last_modified: String = self
            .conn
            .query_row("SELECT LastModified FROM LastModified", [], |r| r.get(0))
            .optional()?
            .unwrap_or_default();
        let version: i64 = self
            .conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))?;
        let now: String = self.conn.query_row(
            "SELECT strftime('%Y-%m-%dT%H:%M:%S+0000', 'now')",
            [],
            |r| r.get(0),
        )?;
        let name = dest
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "backup.jwlibrary".into());
        let manifest = BackupManifest {
            creation_date: now,
            version: 1,
            name,
            kind: 0,
            user_data_backup: BackupInfo {
                last_modified_date: last_modified,
                hash,
                database_name: DB_NAME.into(),
                device_name: device_name.into(),
                schema_version: if version > 0 { version } else { SCHEMA_VERSION },
            },
        };

        let partial = dest.with_extension("jwlibrary.part");
        {
            let mut zip = zip::ZipWriter::new(File::create(&partial)?);
            let options = SimpleFileOptions::default();
            zip.start_file(MANIFEST, options)?;
            zip.write_all(&serde_json::to_vec(&manifest)?)?;
            zip.start_file(DB_NAME, options)?;
            io::copy(&mut File::open(&copy)?, &mut zip)?;
            zip.start_file(THUMBNAIL, options)?;
            zip.write_all(THUMBNAIL_PNG)?;
            // On disk before it replaces an older file of the same name.
            zip.finish()?.sync_all()?;
        }
        fs::rename(&partial, dest)?;
        Ok(())
    }

    /// Replace all data with the backup at `src`. The previous database is
    /// kept next to it as `userData.db.before-restore`.
    pub fn restore_backup(&mut self, src: &Path) -> Result<Summary> {
        let dir = self
            .path
            .parent()
            .map(Path::to_owned)
            .unwrap_or_else(|| PathBuf::from("."));
        let incoming = tempfile::Builder::new()
            .prefix(".restore-")
            .tempfile_in(&dir)?;
        let manifest = unpack_backup(src, incoming.path())?;
        check_database(incoming.path())?;
        {
            let conn = Connection::open(incoming.path())?;
            if conn.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))? == 0 {
                conn.pragma_update(
                    None,
                    "user_version",
                    manifest.user_data_backup.schema_version,
                )?;
            }
        }

        let previous = self.path.with_extension("db.before-restore");
        // Swap files while no connection is open on the old one.
        let placeholder = Connection::open_in_memory()?;
        drop(std::mem::replace(&mut self.conn, placeholder));
        if self.path.exists() {
            fs::rename(&self.path, &previous)?;
        }
        let (_, temp_path) = incoming.keep().map_err(|e| Error::Io(e.error))?;
        fs::rename(&temp_path, &self.path)?;
        *self = Self::open(&self.path)?;
        let mut summary = self.summary()?;
        summary.device = Some(manifest.user_data_backup.device_name);
        Ok(summary)
    }

    /// Merge the backup at `src` into the current data: rows of both sides
    /// are kept, and a row on both sides resolves by recency (docs/FORMAT.md,
    /// "Merging"). The previous database is kept next to it as
    /// `userData.db.before-restore`. On error the data is left unchanged.
    pub fn merge_backup(&mut self, src: &Path) -> Result<MergeReport> {
        let (incoming, device) = self.unpack_incoming(src)?;
        self.write_safety_copy()?;
        Ok(self.merge_file(incoming.path(), device, None)?.0)
    }

    /// Merge another device's sync file at `src` three-way against `base`,
    /// the snapshot of that device's file at the last merge (docs/FORMAT.md,
    /// "Sync"): rows deleted there are deleted here, and a row changed on
    /// one side only keeps that side's version. Without a base this is
    /// [`UserData::merge_backup`]. `guard` decides about the deletions;
    /// [`SyncOutcome::NeedsConfirmation`] writes nothing. The previous
    /// database is kept as `userData.db.before-restore`.
    pub fn sync_merge(
        &mut self,
        src: &Path,
        base: Option<&Snapshot>,
        guard: DeleteGuard,
    ) -> Result<SyncOutcome> {
        let (incoming, device) = self.unpack_incoming(src)?;
        let incoming_rows = rows_of_file(incoming.path())?;
        let snapshot = Snapshot::of(&incoming_rows);
        let resolver = match base {
            None => None,
            Some(base) => {
                let local_rows = rows_of(&self.conn)?;
                let mut classified = classify(base, &snapshot, &Snapshot::of(&local_rows));
                // What is deleted here is gone there, so the merge never
                // touches it again.
                debug_assert!(
                    classified
                        .delete
                        .iter()
                        .all(|k| !snapshot.0.contains_key(k))
                );
                let mass = classified.needs_confirmation(&local_rows);
                match (guard, mass) {
                    (DeleteGuard::Check, Some((deletions, total))) => {
                        return Ok(SyncOutcome::NeedsConfirmation { deletions, total });
                    }
                    (DeleteGuard::Skip, _) => {
                        classified.delete.clear();
                        classified.kept.clear();
                    }
                    (DeleteGuard::Check | DeleteGuard::Apply, _) => {}
                }
                Some(Resolver::new(&incoming_rows, local_rows, classified))
            }
        };
        self.write_safety_copy()?;
        let (report, deleted) = self.merge_file(incoming.path(), device, resolver.as_ref())?;
        Ok(SyncOutcome::Merged(SyncMerge {
            report,
            deleted,
            snapshot,
        }))
    }

    /// Unpack and check the backup at `src` beside the database; returns the
    /// database file and the device the backup was made on.
    fn unpack_incoming(&self, src: &Path) -> Result<(tempfile::NamedTempFile, String)> {
        let dir = self
            .path
            .parent()
            .map(Path::to_owned)
            .unwrap_or_else(|| PathBuf::from("."));
        let incoming = tempfile::Builder::new()
            .prefix(".merge-")
            .tempfile_in(&dir)?;
        let manifest = unpack_backup(src, incoming.path())?;
        check_database(incoming.path())?;
        Ok((incoming, manifest.user_data_backup.device_name))
    }

    /// Merge the database file at `path`; returns the report and the number
    /// of rows deleted (sync only).
    fn merge_file(
        &mut self,
        path: &Path,
        device: String,
        sync: Option<&Resolver>,
    ) -> Result<(MergeReport, i64)> {
        // A merge that broke off earlier may have left `inc` attached.
        let stale: bool = self.conn.query_row(
            "SELECT count(*) > 0 FROM pragma_database_list WHERE name = 'inc'",
            [],
            |r| r.get(0),
        )?;
        if stale {
            let _ = self.conn.execute("DETACH DATABASE inc", []);
        }
        self.conn.execute(
            "ATTACH DATABASE ?1 AS inc",
            [path.to_string_lossy().as_ref()],
        )?;
        let result = self.merge_attached(device, sync);
        // Once the merge is committed (or rolled back), a failing detach
        // changes nothing about the data; the next merge detaches again.
        let _ = self.conn.execute("DETACH DATABASE inc", []);
        result
    }

    /// Copy the database to `userData.db.before-restore`, replacing an older copy.
    fn write_safety_copy(&self) -> Result<()> {
        let copy = self.path.with_extension("db.before-restore");
        // Write beside it and rename, so a failed vacuum keeps the older copy.
        let temp = self.path.with_extension("db.before-restore.tmp");
        match fs::remove_file(&temp) {
            Err(e) if e.kind() != io::ErrorKind::NotFound => return Err(e.into()),
            _ => {}
        }
        let written = self
            .conn
            .execute("VACUUM INTO ?1", [temp.to_string_lossy().as_ref()]);
        if let Err(e) = written {
            let _ = fs::remove_file(&temp);
            return Err(e.into());
        }
        fs::rename(&temp, &copy)?;
        Ok(())
    }

    /// Merge the database attached as `inc` in one transaction, with the
    /// decisions of a sync merge when there are any.
    fn merge_attached(
        &mut self,
        device: String,
        sync: Option<&Resolver>,
    ) -> Result<(MergeReport, i64)> {
        let tx = self.conn.transaction()?;
        let incoming_newer = incoming_is_newer(&tx)?;
        let mut report = MergeReport {
            device: Some(device),
            ..Default::default()
        };
        // Deletions come first. The deleted rows are missing from `inc`, so
        // the merge below never adds them back or updates them, and an
        // incoming highlight equal to a deleted one is added, not matched
        // to the row about to go.
        let deleted = match sync {
            Some(sync) => delete_synced(&tx, sync, &mut report)?,
            None => 0,
        };
        let locations = merge_locations(&tx)?;
        let marks = merge_marks(&tx, &locations, incoming_newer, sync, &mut report)?;
        merge_input_fields(&tx, &locations, incoming_newer, sync, &mut report)?;
        let tags = merge_tags(&tx, sync, &mut report)?;
        merge_notes(&tx, &locations, &marks, &tags, sync, &mut report)?;
        if let Some(sync) = sync {
            merge_note_tags(&tx, &tags, sync, &mut report)?;
        }
        merge_location_tags(&tx, &locations, &tags, sync, &mut report)?;
        merge_bookmarks(&tx, &locations, incoming_newer, sync, &mut report)?;
        if report.added + report.updated + deleted > 0 {
            take_later_clock(&tx)?;
        }
        tx.commit()?;
        Ok((report, deleted))
    }
}

/// Set the current `LastModified` to the backup's when that is later. The
/// clock says when the data were last edited; merging is no edit, and a
/// merge of an old backup must not make this side look newer than it is.
fn take_later_clock(tx: &Transaction<'_>) -> Result<()> {
    tx.execute(
        "UPDATE main.LastModified
         SET LastModified = (SELECT LastModified FROM inc.LastModified LIMIT 1)
         WHERE julianday((SELECT LastModified FROM inc.LastModified LIMIT 1))
               > julianday(LastModified)",
        [],
    )?;
    Ok(())
}

/// Whether the attached database `inc` was changed after the current one,
/// by the database-wide `LastModified`; `None` when equal or unknown.
fn incoming_is_newer(tx: &Transaction<'_>) -> Result<Option<bool>> {
    let diff: Option<f64> = tx.query_row(
        "SELECT julianday((SELECT LastModified FROM inc.LastModified))
              - julianday((SELECT LastModified FROM main.LastModified))",
        [],
        |r| r.get(0),
    )?;
    Ok(match diff {
        Some(d) if d > 0.0 => Some(true),
        Some(d) if d < 0.0 => Some(false),
        _ => None,
    })
}

fn has_table(tx: &Transaction<'_>, schema: &str, table: &str) -> Result<bool> {
    Ok(tx.query_row(
        &format!(
            "SELECT count(*) > 0 FROM {schema}.sqlite_master WHERE type = 'table' AND name = ?1"
        ),
        [table],
        |r| r.get(0),
    )?)
}

fn has_column(tx: &Transaction<'_>, schema: &str, table: &str, column: &str) -> Result<bool> {
    Ok(tx.query_row(
        "SELECT count(*) > 0 FROM pragma_table_info(?1, ?2) WHERE name = ?3",
        params![table, schema, column],
        |r| r.get(0),
    )?)
}

/// What the merge does with an incoming row that differs from the local one.
enum Outcome {
    /// Replace the local row.
    Take,
    /// Keep the local row; no conflict.
    Keep,
    /// Keep the local row and report the incoming one as a conflict.
    Conflict,
}

/// The sync merge's `plan` for a row (rows changed on both sides go to the
/// greater content hash), or, without one, the database-wide recency
/// `incoming_newer`.
fn decide(plan: Option<Plan>, incoming_newer: Option<bool>) -> Outcome {
    match plan.map(|p| p.side) {
        Some(Side::Incoming) => Outcome::Take,
        Some(Side::Local) => Outcome::Keep,
        Some(Side::Recency { incoming_wins_tie }) => tie_break(incoming_wins_tie),
        None if incoming_newer == Some(true) => Outcome::Take,
        None => Outcome::Conflict,
    }
}

/// A sync merge's choice between two versions changed at the same time:
/// take the incoming one, or keep the local one and list the other.
fn tie_break(incoming_wins: bool) -> Outcome {
    if incoming_wins {
        Outcome::Take
    } else {
        Outcome::Conflict
    }
}

/// The sync merge reports the row as a conflict whichever side wins.
fn reported(plan: Option<Plan>) -> bool {
    plan.is_some_and(|p| p.report)
}

/// The sync merge keeps a row missing here deleted.
fn deleted_here(plan: Option<Plan>) -> bool {
    plan.is_some_and(|p| p.side == Side::Local)
}

/// The sync plan for incoming row `row` matched to local row `local`.
fn plan_of(sync: Option<&Resolver>, row: RowRef, local: Option<RowRef>) -> Option<Plan> {
    sync.and_then(|s| s.plan(&row, local.as_ref()))
}

/// Delete the local rows the other device deleted, through the editor's
/// deletion paths, and report the rows kept because they changed here.
/// Returns how many rows were deleted.
fn delete_synced(tx: &Transaction<'_>, sync: &Resolver, report: &mut MergeReport) -> Result<i64> {
    // Tag maps first, then notes and highlights; tags last, once nothing
    // refers to them.
    let order = |key: &RowKey| match key {
        RowKey::TagMap(..) => 0,
        RowKey::Note(_) => 1,
        RowKey::Mark(_) => 2,
        RowKey::Bookmark(..) | RowKey::InputField(..) => 3,
        RowKey::Tag(..) => 4,
    };
    let mut keys: Vec<&RowKey> = sync.classified.delete.iter().collect();
    keys.sort_by_key(|k| order(k));
    let mut deleted = 0;
    for key in keys {
        // Only the row the snapshot was read from: a second row with the
        // same key was never seen there.
        let Some(row) = sync.local_row(key) else {
            continue;
        };
        match row {
            RowRef::TagMap(id) => {
                tx.execute("DELETE FROM main.TagMap WHERE TagMapId = ?1", [id])?;
            }
            RowRef::Note(id) => delete_note_row(tx, *id)?,
            RowRef::Mark(id) => delete_mark_row(tx, *id)?,
            RowRef::Bookmark(id) => {
                tx.execute("DELETE FROM main.Bookmark WHERE BookmarkId = ?1", [id])?;
            }
            // On every location of the page, as the editor deletes an
            // answer: a value left on another one would show up again.
            RowRef::InputField(location, tag) => {
                tx.execute(
                    &format!(
                        "DELETE FROM main.InputField WHERE TextTag = ?2 AND LocationId IN (
                             SELECT m.LocationId FROM main.Location m, main.Location l
                             WHERE l.LocationId = ?1
                               AND (m.LocationId = l.LocationId OR {ANSWER_SIBLINGS}))"
                    ),
                    params![location, tag],
                )?;
            }
            RowRef::Tag(id) => {
                // Still in use (e.g. by a playlist): kept, like an edit.
                let used: bool = tx.query_row(
                    "SELECT count(*) > 0 FROM main.TagMap WHERE TagId = ?1",
                    [id],
                    |r| r.get(0),
                )?;
                if used {
                    report.conflicts.push(describe_local(tx, row)?);
                    continue;
                }
                tx.execute("DELETE FROM main.Tag WHERE TagId = ?1", [id])?;
            }
        }
        deleted += 1;
    }
    for key in &sync.classified.kept {
        if let Some(row) = sync.local_row(key) {
            report.conflicts.push(describe_local(tx, row)?);
        }
    }
    Ok(deleted)
}

/// A local row as a conflict: one deleted in the backup and kept here.
fn describe_local(tx: &Transaction<'_>, row: &RowRef) -> Result<Conflict> {
    Ok(match row {
        RowRef::Mark(id) => {
            let (guid, location, color): (String, i64, i64) = tx.query_row(
                "SELECT UserMarkGuid, LocationId, ColorIndex FROM main.UserMark
                 WHERE UserMarkId = ?1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )?;
            let summary = format!("{}, color {color}", location_label(tx, location)?);
            conflict("UserMark", &guid, summary)
        }
        RowRef::Note(id) => {
            let (guid, title, content): (String, Option<String>, Option<String>) = tx.query_row(
                "SELECT Guid, Title, Content FROM main.Note WHERE NoteId = ?1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )?;
            conflict("Note", &guid, note_summary(title, content))
        }
        RowRef::Tag(id) => {
            let name: String =
                tx.query_row("SELECT Name FROM main.Tag WHERE TagId = ?1", [id], |r| {
                    r.get(0)
                })?;
            conflict("Tag", &name, name.clone())
        }
        RowRef::Bookmark(id) => {
            let (publication, slot, title): (i64, i64, String) = tx.query_row(
                "SELECT PublicationLocationId, Slot, Title FROM main.Bookmark
                 WHERE BookmarkId = ?1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )?;
            conflict("Bookmark", &format!("{publication}:{slot}"), title)
        }
        RowRef::InputField(location, tag) => {
            let summary = format!("{}, field {tag}", location_label(tx, *location)?);
            conflict("InputField", &format!("{location}:{tag}"), summary)
        }
        RowRef::TagMap(id) => {
            let name: String = tx.query_row(
                "SELECT t.Name FROM main.TagMap m JOIN main.Tag t ON t.TagId = m.TagId
                 WHERE m.TagMapId = ?1",
                [id],
                |r| r.get(0),
            )?;
            conflict("TagMap", &id.to_string(), name)
        }
    })
}

/// Map every incoming `Location` to the local one with the same key, adding
/// the missing ones. Returns incoming `LocationId` → local `LocationId`.
fn merge_locations(tx: &Transaction<'_>) -> Result<HashMap<i64, i64>> {
    let mut extra = Vec::new();
    for column in ["Title", "Specialty", "Edition"] {
        if has_column(tx, "inc", "Location", column)? && has_column(tx, "main", "Location", column)?
        {
            extra.push(column);
        }
    }
    let extra: String = extra.iter().map(|c| format!(", {c}")).collect();
    let ids: Vec<i64> = {
        let mut stmt = tx.prepare("SELECT LocationId FROM inc.Location ORDER BY LocationId")?;
        let rows = stmt.query_map([], |r| r.get(0))?;
        rows.collect::<rusqlite::Result<_>>()?
    };
    let mut map = HashMap::with_capacity(ids.len());
    for id in ids {
        let same_key = |condition: &str| -> Result<Option<i64>> {
            Ok(tx
                .query_row(
                    &format!(
                        "SELECT m.LocationId FROM main.Location m, inc.Location i
                         WHERE i.LocationId = ?1 AND {condition}
                         ORDER BY m.LocationId LIMIT 1"
                    ),
                    [id],
                    |r| r.get(0),
                )
                .optional()?)
        };
        // The same page as the editor finds it (`UserData::location_id`: a
        // missing book, chapter or document is 0); failing that, a row with
        // the same value in one of the UNIQUE keys JW Library uses (which
        // only bind when none of their columns is NULL).
        let found = match same_key(
            "m.Type = i.Type AND m.Track IS i.Track AND m.KeySymbol IS i.KeySymbol
             AND m.MepsLanguage IS i.MepsLanguage AND m.IssueTagNumber IS i.IssueTagNumber
             AND IFNULL(m.DocumentId, 0) = IFNULL(i.DocumentId, 0)
             AND IFNULL(m.BookNumber, 0) = IFNULL(i.BookNumber, 0)
             AND IFNULL(m.ChapterNumber, 0) = IFNULL(i.ChapterNumber, 0)",
        )? {
            Some(found) => Some(found),
            None => match same_key(
                "m.BookNumber = i.BookNumber AND m.ChapterNumber = i.ChapterNumber
                 AND m.KeySymbol = i.KeySymbol AND m.MepsLanguage = i.MepsLanguage
                 AND m.Type = i.Type",
            )? {
                Some(found) => Some(found),
                None => same_key(
                    "m.KeySymbol = i.KeySymbol AND m.IssueTagNumber = i.IssueTagNumber
                     AND m.MepsLanguage = i.MepsLanguage AND m.DocumentId = i.DocumentId
                     AND m.Track = i.Track AND m.Type = i.Type",
                )?,
            },
        };
        let local = match found {
            Some(local) => {
                if extra.contains("Title") {
                    tx.execute(
                        "UPDATE main.Location
                         SET Title = (SELECT Title FROM inc.Location WHERE LocationId = ?2)
                         WHERE LocationId = ?1 AND Title IS NULL",
                        [local, id],
                    )?;
                }
                local
            }
            None => {
                tx.execute(
                    &format!(
                        "INSERT INTO main.Location (BookNumber, ChapterNumber, DocumentId, Track,
                             IssueTagNumber, KeySymbol, MepsLanguage, Type{extra})
                         SELECT BookNumber, ChapterNumber, DocumentId, Track,
                             IssueTagNumber, KeySymbol, MepsLanguage, Type{extra}
                         FROM inc.Location WHERE LocationId = ?1"
                    ),
                    [id],
                )?;
                tx.last_insert_rowid()
            }
        };
        map.insert(id, local);
    }
    Ok(map)
}

/// A `BlockRange` row: block type, identifier, start and end token.
type RangeRow = (i64, i64, Option<i64>, Option<i64>);

fn range_of(row: &RangeRow) -> Range {
    Range {
        block_type: row.0,
        identifier: row.1,
        start: row.2.unwrap_or(0),
        end: row.3.unwrap_or(0),
    }
}

/// The ranges of a mark in `schema` (`main` or `inc`), sorted.
fn mark_ranges(tx: &Transaction<'_>, schema: &str, mark: i64) -> Result<Vec<RangeRow>> {
    let mut stmt = tx.prepare(&format!(
        "SELECT BlockType, Identifier, StartToken, EndToken FROM {schema}.BlockRange
         WHERE UserMarkId = ?1"
    ))?;
    let rows = stmt.query_map([mark], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?;
    let mut ranges: Vec<RangeRow> = rows.collect::<rusqlite::Result<_>>()?;
    ranges.sort_unstable();
    Ok(ranges)
}

/// Whether a local highlight on `location` covers a token of `ranges`.
fn overlaps_local(tx: &Transaction<'_>, location: i64, ranges: &[RangeRow]) -> Result<bool> {
    for row in ranges {
        let r = range_of(row);
        let hit: bool = tx.query_row(
            "SELECT count(*) > 0 FROM main.BlockRange b
             JOIN main.UserMark m ON m.UserMarkId = b.UserMarkId
             WHERE m.LocationId = ?1 AND b.BlockType = ?2 AND b.Identifier = ?3
               AND IFNULL(b.StartToken, 0) <= ?5 AND IFNULL(b.EndToken, 0) >= ?4",
            params![location, r.block_type, r.identifier, r.start, r.end],
            |r| r.get(0),
        )?;
        if hit {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Give `mark` the `ranges`, cutting them out of other highlights first.
fn put_ranges(tx: &Transaction<'_>, mark: i64, location: i64, ranges: &[RangeRow]) -> Result<()> {
    tx.execute("DELETE FROM main.BlockRange WHERE UserMarkId = ?1", [mark])?;
    for row in ranges {
        cut_ranges(tx, location, &range_of(row))?;
    }
    for (block_type, identifier, start, end) in ranges {
        tx.execute(
            "INSERT INTO main.BlockRange (BlockType, Identifier, StartToken, EndToken, UserMarkId)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![block_type, identifier, start, end, mark],
        )?;
    }
    Ok(())
}

/// How a location is shown in a conflict: its title, or its key.
fn location_label(tx: &Transaction<'_>, location: i64) -> Result<String> {
    Ok(tx.query_row(
        "SELECT COALESCE(Title, IFNULL(KeySymbol, '') || ' '
                || COALESCE(BookNumber || ':' || ChapterNumber, DocumentId, ''))
         FROM main.Location WHERE LocationId = ?1",
        [location],
        |r| r.get(0),
    )?)
}

/// How a backup highlight is shown in a conflict: where it is and its color.
fn mark_summary(tx: &Transaction<'_>, location: i64, color: i64) -> Result<String> {
    Ok(format!(
        "{}, color {color} in the backup",
        location_label(tx, location)?
    ))
}

/// A local highlight on `location` with the same color and exactly the same
/// ranges, made independently on both sides.
fn identical_local_mark(
    tx: &Transaction<'_>,
    location: i64,
    color: i64,
    ranges: &[RangeRow],
) -> Result<Option<i64>> {
    let ids: Vec<i64> = {
        let mut stmt = tx.prepare(
            "SELECT UserMarkId FROM main.UserMark WHERE LocationId = ?1 AND ColorIndex = ?2
             ORDER BY UserMarkId",
        )?;
        let rows = stmt.query_map([location, color], |r| r.get(0))?;
        rows.collect::<rusqlite::Result<_>>()?
    };
    for id in ids {
        if mark_ranges(tx, "main", id)? == ranges {
            return Ok(Some(id));
        }
    }
    Ok(None)
}

fn conflict(table: &str, guid: &str, summary: String) -> Conflict {
    Conflict {
        table: table.into(),
        guid: guid.into(),
        summary,
    }
}

/// An incoming `UserMark` row.
struct IncomingMark {
    id: i64,
    guid: String,
    color: i64,
    location: i64,
    style: i64,
    version: i64,
}

/// Merge `inc.UserMark` with its ranges, matched by `UserMarkGuid`. Returns
/// guid → local `UserMarkId` of every incoming mark present after the merge,
/// for rows that refer to marks.
fn merge_marks(
    tx: &Transaction<'_>,
    locations: &HashMap<i64, i64>,
    incoming_newer: Option<bool>,
    sync: Option<&Resolver>,
    report: &mut MergeReport,
) -> Result<HashMap<String, i64>> {
    let incoming: Vec<IncomingMark> = {
        let mut stmt = tx.prepare(
            "SELECT UserMarkId, UserMarkGuid, ColorIndex, LocationId, StyleIndex, Version
             FROM inc.UserMark ORDER BY UserMarkId",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(IncomingMark {
                id: r.get(0)?,
                guid: r.get(1)?,
                color: r.get(2)?,
                location: r.get(3)?,
                style: r.get(4)?,
                version: r.get(5)?,
            })
        })?;
        rows.collect::<rusqlite::Result<_>>()?
    };
    let mut map = HashMap::with_capacity(incoming.len());
    for mark in incoming {
        // A mark on a missing location is broken in the backup.
        let Some(&location) = locations.get(&mark.location) else {
            continue;
        };
        let ranges = mark_ranges(tx, "inc", mark.id)?;
        let local: Option<(i64, i64, i64)> = tx
            .query_row(
                "SELECT UserMarkId, ColorIndex, LocationId FROM main.UserMark
                 WHERE UserMarkGuid = ?1",
                [&mark.guid],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()?;
        let plan = plan_of(
            sync,
            RowRef::Mark(mark.id),
            local.map(|(id, ..)| RowRef::Mark(id)),
        );
        match local {
            None if deleted_here(plan) => report.unchanged += 1,
            None => {
                // The same highlight made on both sides is no conflict; rows
                // referring to the backup's one use the local one.
                if let Some(twin) = identical_local_mark(tx, location, mark.color, &ranges)? {
                    map.insert(mark.guid, twin);
                    report.unchanged += 1;
                    continue;
                }
                // A token has one color: an overlap goes to the newer side.
                if incoming_newer != Some(true) && overlaps_local(tx, location, &ranges)? {
                    let summary = mark_summary(tx, location, mark.color)?;
                    report
                        .conflicts
                        .push(conflict("UserMark", &mark.guid, summary));
                    continue;
                }
                tx.execute(
                    "INSERT INTO main.UserMark (ColorIndex, LocationId, StyleIndex, UserMarkGuid,
                                                Version)
                     VALUES (?1, ?2, ?3, ?4, ?5)",
                    params![mark.color, location, mark.style, mark.guid, mark.version],
                )?;
                let id = tx.last_insert_rowid();
                put_ranges(tx, id, location, &ranges)?;
                report.added += 1;
                if reported(plan) {
                    let summary = mark_summary(tx, location, mark.color)?;
                    report
                        .conflicts
                        .push(conflict("UserMark", &mark.guid, summary));
                }
                map.insert(mark.guid, id);
            }
            Some((id, color, local_location)) => {
                map.insert(mark.guid.clone(), id);
                let same = color == mark.color
                    && local_location == location
                    && mark_ranges(tx, "main", id)? == ranges;
                if same {
                    report.unchanged += 1;
                    continue;
                }
                match decide(plan, incoming_newer) {
                    Outcome::Take => {
                        tx.execute(
                            "UPDATE main.UserMark SET ColorIndex = ?1, LocationId = ?2,
                                                      StyleIndex = ?3
                             WHERE UserMarkId = ?4",
                            params![mark.color, location, mark.style, id],
                        )?;
                        put_ranges(tx, id, location, &ranges)?;
                        report.updated += 1;
                        if reported(plan) {
                            let summary = mark_summary(tx, location, mark.color)?;
                            report
                                .conflicts
                                .push(conflict("UserMark", &mark.guid, summary));
                        }
                    }
                    Outcome::Keep => report.unchanged += 1,
                    // The backup is older or as old: keep the local mark,
                    // but say so, since the backup's may be the later edit.
                    Outcome::Conflict => {
                        let summary = mark_summary(tx, local_location, mark.color)?;
                        report
                            .conflicts
                            .push(conflict("UserMark", &mark.guid, summary));
                    }
                }
            }
        }
    }
    // Cutting overlaps can remove a whole local mark that was mapped earlier.
    let mut gone = Vec::new();
    for (guid, id) in &map {
        let exists: bool = tx.query_row(
            "SELECT count(*) > 0 FROM main.UserMark WHERE UserMarkId = ?1",
            [id],
            |r| r.get(0),
        )?;
        if !exists {
            gone.push(guid.clone());
        }
    }
    for guid in gone {
        map.remove(&guid);
    }
    Ok(map)
}

/// Locations `m` and `l` hold the answers of the same page, as the editor
/// reads them (`UserData::answer_location_ids`): a location without a
/// language belongs to the page in every language.
pub(crate) const ANSWER_SIBLINGS: &str =
    "m.Type = 0 AND l.Type = 0 AND m.Track IS NULL AND l.Track IS NULL
    AND m.KeySymbol = l.KeySymbol AND m.IssueTagNumber = l.IssueTagNumber
    AND IFNULL(m.DocumentId, 0) = IFNULL(l.DocumentId, 0)
    AND IFNULL(m.BookNumber, 0) = IFNULL(l.BookNumber, 0)
    AND IFNULL(m.ChapterNumber, 0) = IFNULL(l.ChapterNumber, 0)
    AND (m.MepsLanguage IS NULL OR l.MepsLanguage IS NULL OR m.MepsLanguage = l.MepsLanguage)";

/// Merge `inc.InputField`, matched by page and `TextTag` against the row
/// the editor shows.
fn merge_input_fields(
    tx: &Transaction<'_>,
    locations: &HashMap<i64, i64>,
    incoming_newer: Option<bool>,
    sync: Option<&Resolver>,
    report: &mut MergeReport,
) -> Result<()> {
    // Not every backup has answer fields.
    if !has_table(tx, "inc", "InputField")? || !has_table(tx, "main", "InputField")? {
        return Ok(());
    }
    // (LocationId, TextTag, Value, hidden): a field the backup also has on
    // an earlier location of the page is not shown there, and is left out.
    let incoming: Vec<(i64, String, String, bool)> = {
        let mut stmt = tx.prepare(&format!(
            "SELECT f.LocationId, f.TextTag, f.Value, EXISTS (
                 SELECT 1 FROM inc.InputField g
                 JOIN inc.Location m ON m.LocationId = g.LocationId
                 JOIN inc.Location l ON l.LocationId = f.LocationId
                 WHERE g.TextTag = f.TextTag AND g.LocationId < f.LocationId
                   AND {ANSWER_SIBLINGS})
             FROM inc.InputField f ORDER BY f.LocationId, f.TextTag"
        ))?;
        let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?;
        rows.collect::<rusqlite::Result<_>>()?
    };
    for (incoming_location, tag, value, hidden) in incoming {
        let Some(&location) = locations.get(&incoming_location) else {
            continue;
        };
        if hidden {
            continue;
        }
        // The row the editor shows for this field (`UserData::answers`): the
        // first of the page's locations.
        let local: Option<(i64, String)> = tx
            .query_row(
                &format!(
                    "SELECT f.LocationId, f.Value FROM main.InputField f
                     JOIN main.Location m ON m.LocationId = f.LocationId
                     JOIN main.Location l ON l.LocationId = ?1
                     WHERE f.TextTag = ?2 AND (m.LocationId = l.LocationId OR {ANSWER_SIBLINGS})
                     ORDER BY f.LocationId LIMIT 1"
                ),
                params![location, tag],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        let plan = plan_of(
            sync,
            RowRef::InputField(incoming_location, tag.clone()),
            local
                .as_ref()
                .map(|(shown, _)| RowRef::InputField(*shown, tag.clone())),
        );
        let field_conflict = |shown: i64| -> Result<Conflict> {
            let summary = format!("{}, field {tag}", location_label(tx, shown)?);
            Ok(conflict("InputField", &format!("{shown}:{tag}"), summary))
        };
        match local {
            None if deleted_here(plan) => report.unchanged += 1,
            None => {
                tx.execute(
                    "INSERT INTO main.InputField (LocationId, TextTag, Value) VALUES (?1, ?2, ?3)",
                    params![location, tag, value],
                )?;
                report.added += 1;
                if reported(plan) {
                    report.conflicts.push(field_conflict(location)?);
                }
            }
            Some((_, local)) if local == value => report.unchanged += 1,
            Some((shown, _)) => match decide(plan, incoming_newer) {
                Outcome::Take => {
                    tx.execute(
                        "UPDATE main.InputField SET Value = ?3
                         WHERE LocationId = ?1 AND TextTag = ?2",
                        params![shown, tag, value],
                    )?;
                    report.updated += 1;
                    if reported(plan) {
                        report.conflicts.push(field_conflict(shown)?);
                    }
                }
                Outcome::Keep => report.unchanged += 1,
                // The backup is older or as old: keep the local text, but say so.
                Outcome::Conflict => report.conflicts.push(field_conflict(shown)?),
            },
        }
    }
    Ok(())
}

/// Map every incoming `Tag` to the local one with the same type and name,
/// adding the missing ones (but not those the sync merge keeps deleted).
/// Returns incoming `TagId` → local `TagId`.
fn merge_tags(
    tx: &Transaction<'_>,
    sync: Option<&Resolver>,
    report: &mut MergeReport,
) -> Result<HashMap<i64, i64>> {
    let incoming: Vec<(i64, i64, String)> = {
        let mut stmt = tx.prepare("SELECT TagId, Type, Name FROM inc.Tag ORDER BY TagId")?;
        let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
        rows.collect::<rusqlite::Result<_>>()?
    };
    let mut map = HashMap::with_capacity(incoming.len());
    for (id, kind, name) in incoming {
        let local: Option<i64> = tx
            .query_row(
                "SELECT TagId FROM main.Tag WHERE Type = ?1 AND Name = ?2",
                params![kind, name],
                |r| r.get(0),
            )
            .optional()?;
        let plan = plan_of(sync, RowRef::Tag(id), local.map(RowRef::Tag));
        let local = match local {
            Some(local) => local,
            None if deleted_here(plan) => continue,
            None => {
                tx.execute(
                    "INSERT INTO main.Tag (Type, Name) VALUES (?1, ?2)",
                    params![kind, name],
                )?;
                if reported(plan) {
                    report.conflicts.push(conflict("Tag", &name, name.clone()));
                }
                tx.last_insert_rowid()
            }
        };
        map.insert(id, local);
    }
    Ok(map)
}

/// Give note `note` exactly the local tags `tags`, each at the next free
/// position of its tag.
fn put_note_tags(tx: &Transaction<'_>, note: i64, tags: &[i64]) -> Result<()> {
    tx.execute("DELETE FROM main.TagMap WHERE NoteId = ?1", [note])?;
    for tag in tags {
        tx.execute(
            "INSERT OR IGNORE INTO main.TagMap (NoteId, TagId, Position)
             VALUES (?1, ?2, (SELECT IFNULL(max(Position), -1) + 1
                              FROM main.TagMap WHERE TagId = ?2))",
            [note, *tag],
        )?;
    }
    Ok(())
}

/// The local tags of incoming note `note`, in their incoming order.
fn incoming_note_tags(
    tx: &Transaction<'_>,
    tags: &HashMap<i64, i64>,
    note: i64,
) -> Result<Vec<i64>> {
    let mut stmt =
        tx.prepare("SELECT TagId FROM inc.TagMap WHERE NoteId = ?1 ORDER BY Position, TagMapId")?;
    let rows = stmt.query_map([note], |r| r.get::<_, i64>(0))?;
    let incoming: Vec<i64> = rows.collect::<rusqlite::Result<_>>()?;
    Ok(incoming
        .into_iter()
        .filter_map(|t| tags.get(&t).copied())
        .collect())
}

/// An incoming `Note` row with its references mapped to local ids.
struct IncomingNote {
    id: i64,
    guid: String,
    mark: Option<i64>,
    /// The note has a highlight in the backup, though `mark` may have no
    /// local one (skipped as a conflict, or cut away).
    had_mark: bool,
    location: Option<i64>,
    /// The note has a location in the backup that was not imported.
    had_location: bool,
}

/// How a note is shown in a conflict: its title, or the start of its text.
fn note_summary(title: Option<String>, content: Option<String>) -> String {
    const MAX: usize = 40;
    if let Some(title) = title.filter(|t| !t.trim().is_empty()) {
        return title;
    }
    let text = content
        .unwrap_or_default()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    if text.chars().count() > MAX {
        text.chars().take(MAX).collect::<String>() + "…"
    } else {
        text
    }
}

/// Merge `inc.Note`, matched by `Guid`; the note's own `LastModified`
/// decides. A manual merge takes a note's tags with it; a sync merge
/// leaves them to [`merge_note_tags`].
fn merge_notes(
    tx: &Transaction<'_>,
    locations: &HashMap<i64, i64>,
    marks: &HashMap<String, i64>,
    tags: &HashMap<i64, i64>,
    sync: Option<&Resolver>,
    report: &mut MergeReport,
) -> Result<()> {
    let incoming: Vec<IncomingNote> = {
        let mut stmt = tx.prepare(
            "SELECT n.NoteId, n.Guid, u.UserMarkGuid, n.LocationId, n.UserMarkId IS NOT NULL,
                    n.LocationId IS NOT NULL
             FROM inc.Note n
             LEFT JOIN inc.UserMark u ON u.UserMarkId = n.UserMarkId
             ORDER BY n.NoteId",
        )?;
        let rows = stmt.query_map([], |r| {
            let mark: Option<String> = r.get(2)?;
            let location: Option<i64> = r.get(3)?;
            Ok(IncomingNote {
                id: r.get(0)?,
                guid: r.get(1)?,
                // A skipped mark or a missing location leaves the note without.
                mark: mark.and_then(|g| marks.get(&g).copied()),
                had_mark: r.get(4)?,
                location: location.and_then(|l| locations.get(&l).copied()),
                had_location: r.get(5)?,
            })
        })?;
        rows.collect::<rusqlite::Result<_>>()?
    };
    for note in incoming {
        // (NoteId, how much newer the incoming note is, same text, title, content)
        type LocalNote = (i64, Option<f64>, bool, Option<String>, Option<String>);
        let local: Option<LocalNote> = tx
            .query_row(
                "SELECT m.NoteId, julianday(i.LastModified) - julianday(m.LastModified),
                        m.Title IS i.Title AND m.Content IS i.Content, m.Title, m.Content
                 FROM main.Note m, inc.Note i WHERE i.NoteId = ?1 AND m.Guid = i.Guid",
                [note.id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
            )
            .optional()?;
        let plan = plan_of(
            sync,
            RowRef::Note(note.id),
            local.as_ref().map(|l| RowRef::Note(l.0)),
        );
        let Some((id, diff, same_text, title, content)) = local else {
            if deleted_here(plan) {
                report.unchanged += 1;
                continue;
            }
            tx.execute(
                "INSERT INTO main.Note (Guid, UserMarkId, LocationId, Title, Content,
                                        LastModified, Created, BlockType, BlockIdentifier)
                 SELECT Guid, ?2, ?3, Title, Content, LastModified, Created, BlockType,
                        BlockIdentifier
                 FROM inc.Note WHERE NoteId = ?1",
                params![note.id, note.mark, note.location],
            )?;
            let id = tx.last_insert_rowid();
            if sync.is_none() {
                put_note_tags(tx, id, &incoming_note_tags(tx, tags, note.id)?)?;
            }
            report.added += 1;
            if reported(plan) {
                let (title, content) = tx.query_row(
                    "SELECT Title, Content FROM main.Note WHERE NoteId = ?1",
                    [id],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )?;
                report
                    .conflicts
                    .push(conflict("Note", &note.guid, note_summary(title, content)));
            }
            continue;
        };
        let by_time = match diff {
            Some(diff) if diff > 0.0 => Outcome::Take,
            Some(diff) if diff < 0.0 => Outcome::Keep,
            _ if same_text => Outcome::Keep,
            _ => Outcome::Conflict,
        };
        let outcome = match plan.map(|p| p.side) {
            Some(Side::Incoming) => Outcome::Take,
            Some(Side::Local) => Outcome::Keep,
            // Changed at the same time: every computer keeps the same one.
            Some(Side::Recency { incoming_wins_tie }) if matches!(diff, None | Some(0.0)) => {
                tie_break(incoming_wins_tie)
            }
            Some(Side::Recency { .. }) | None => by_time,
        };
        match outcome {
            Outcome::Take => {
                // A backup highlight that did not make it here leaves the
                // note on its local one rather than on none.
                let keep_mark = note.had_mark && note.mark.is_none();
                let keep_location = note.had_location && note.location.is_none();
                // The place in the text belongs to the location, so it stays with it.
                tx.execute(
                    "UPDATE main.Note
                     SET (Title, Content, LastModified) =
                         (SELECT Title, Content, LastModified FROM inc.Note WHERE NoteId = ?1),
                         BlockType = CASE WHEN ?6 THEN BlockType
                                     ELSE (SELECT BlockType FROM inc.Note WHERE NoteId = ?1) END,
                         BlockIdentifier = CASE WHEN ?6 THEN BlockIdentifier
                                     ELSE (SELECT BlockIdentifier FROM inc.Note WHERE NoteId = ?1) END,
                         UserMarkId = CASE WHEN ?5 THEN UserMarkId ELSE ?2 END,
                         LocationId = CASE WHEN ?6 THEN LocationId ELSE ?3 END
                     WHERE NoteId = ?4",
                    params![
                        note.id,
                        note.mark,
                        note.location,
                        id,
                        keep_mark,
                        keep_location
                    ],
                )?;
                if sync.is_none() {
                    put_note_tags(tx, id, &incoming_note_tags(tx, tags, note.id)?)?;
                }
                report.updated += 1;
                if reported(plan) {
                    report.conflicts.push(conflict(
                        "Note",
                        &note.guid,
                        note_summary(title, content),
                    ));
                }
            }
            Outcome::Keep if reported(plan) => {
                report
                    .conflicts
                    .push(conflict("Note", &note.guid, note_summary(title, content)));
            }
            Outcome::Keep => report.unchanged += 1,
            Outcome::Conflict => {
                report
                    .conflicts
                    .push(conflict("Note", &note.guid, note_summary(title, content)));
            }
        }
    }
    Ok(())
}

/// Merge the tags of notes one by one (sync): a note's tag that is missing
/// here is added, unless it was removed here. The order of a tag's notes
/// is left as it is here. Notes the merge did not bring in are skipped.
fn merge_note_tags(
    tx: &Transaction<'_>,
    tags: &HashMap<i64, i64>,
    sync: &Resolver,
    report: &mut MergeReport,
) -> Result<()> {
    let incoming: Vec<(i64, i64, String)> = {
        let mut stmt = tx.prepare(
            "SELECT m.TagMapId, m.TagId, n.Guid FROM inc.TagMap m
             JOIN inc.Note n ON n.NoteId = m.NoteId
             WHERE m.PlaylistItemId IS NULL
             ORDER BY m.TagId, m.Position, m.TagMapId",
        )?;
        let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
        rows.collect::<rusqlite::Result<_>>()?
    };
    for (id, tag, guid) in incoming {
        let Some(&tag) = tags.get(&tag) else {
            continue;
        };
        let note: Option<i64> = tx
            .query_row(
                "SELECT NoteId FROM main.Note WHERE Guid = ?1",
                [&guid],
                |r| r.get(0),
            )
            .optional()?;
        let Some(note) = note else {
            continue;
        };
        let local: Option<i64> = tx
            .query_row(
                "SELECT TagMapId FROM main.TagMap WHERE TagId = ?1 AND NoteId = ?2",
                [tag, note],
                |r| r.get(0),
            )
            .optional()?;
        let plan = sync.plan(&RowRef::TagMap(id), local.map(RowRef::TagMap).as_ref());
        if local.is_some() || deleted_here(plan) {
            report.unchanged += 1;
            continue;
        }
        tx.execute(
            "INSERT INTO main.TagMap (NoteId, TagId, Position)
             VALUES (?1, ?2, (SELECT IFNULL(max(Position), -1) + 1
                              FROM main.TagMap WHERE TagId = ?2))",
            [note, tag],
        )?;
        report.added += 1;
    }
    Ok(())
}

/// Add the incoming tags of locations (favorites) that are missing here.
fn merge_location_tags(
    tx: &Transaction<'_>,
    locations: &HashMap<i64, i64>,
    tags: &HashMap<i64, i64>,
    sync: Option<&Resolver>,
    report: &mut MergeReport,
) -> Result<()> {
    let incoming: Vec<(i64, i64, i64)> = {
        let mut stmt = tx.prepare(
            "SELECT TagMapId, TagId, LocationId FROM inc.TagMap
             WHERE NoteId IS NULL AND PlaylistItemId IS NULL AND LocationId IS NOT NULL
             ORDER BY TagId, Position",
        )?;
        let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
        rows.collect::<rusqlite::Result<_>>()?
    };
    for (id, tag, location) in incoming {
        let (Some(&tag), Some(&location)) = (tags.get(&tag), locations.get(&location)) else {
            continue;
        };
        if let Some(sync) = sync {
            let local: Option<i64> = tx
                .query_row(
                    "SELECT TagMapId FROM main.TagMap WHERE TagId = ?1 AND LocationId = ?2",
                    [tag, location],
                    |r| r.get(0),
                )
                .optional()?;
            let plan = sync.plan(&RowRef::TagMap(id), local.map(RowRef::TagMap).as_ref());
            if local.is_none() && deleted_here(plan) {
                report.unchanged += 1;
                continue;
            }
        }
        let inserted = tx.execute(
            "INSERT OR IGNORE INTO main.TagMap (LocationId, TagId, Position)
             VALUES (?1, ?2, (SELECT IFNULL(max(Position), -1) + 1
                              FROM main.TagMap WHERE TagId = ?2))",
            [location, tag],
        )?;
        if inserted > 0 {
            report.added += 1;
        } else {
            report.unchanged += 1;
        }
    }
    Ok(())
}

/// Merge `inc.Bookmark`, matched by (publication, slot).
fn merge_bookmarks(
    tx: &Transaction<'_>,
    locations: &HashMap<i64, i64>,
    incoming_newer: Option<bool>,
    sync: Option<&Resolver>,
    report: &mut MergeReport,
) -> Result<()> {
    let incoming: Vec<(i64, i64, i64, i64)> = {
        let mut stmt = tx.prepare(
            "SELECT BookmarkId, LocationId, PublicationLocationId, Slot FROM inc.Bookmark
             ORDER BY BookmarkId",
        )?;
        let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?;
        rows.collect::<rusqlite::Result<_>>()?
    };
    for (id, location, publication, slot) in incoming {
        let (Some(&location), Some(&publication)) =
            (locations.get(&location), locations.get(&publication))
        else {
            continue;
        };
        // (BookmarkId, same target and text, title)
        let local: Option<(i64, bool, String)> = tx
            .query_row(
                "SELECT m.BookmarkId,
                        m.LocationId = ?2 AND m.Title IS i.Title AND m.Snippet IS i.Snippet
                        AND m.BlockType IS i.BlockType AND m.BlockIdentifier IS i.BlockIdentifier,
                        m.Title
                 FROM main.Bookmark m, inc.Bookmark i
                 WHERE i.BookmarkId = ?1 AND m.PublicationLocationId = ?3 AND m.Slot = ?4",
                params![id, location, publication, slot],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()?;
        let plan = plan_of(
            sync,
            RowRef::Bookmark(id),
            local.as_ref().map(|(l, ..)| RowRef::Bookmark(*l)),
        );
        let key = format!("{publication}:{slot}");
        match local {
            None if deleted_here(plan) => report.unchanged += 1,
            None => {
                tx.execute(
                    "INSERT INTO main.Bookmark (LocationId, PublicationLocationId, Slot, Title,
                                                Snippet, BlockType, BlockIdentifier)
                     SELECT ?2, ?3, Slot, Title, Snippet, BlockType, BlockIdentifier
                     FROM inc.Bookmark WHERE BookmarkId = ?1",
                    params![id, location, publication],
                )?;
                report.added += 1;
                if reported(plan) {
                    let title: String = tx.query_row(
                        "SELECT Title FROM inc.Bookmark WHERE BookmarkId = ?1",
                        [id],
                        |r| r.get(0),
                    )?;
                    report.conflicts.push(conflict("Bookmark", &key, title));
                }
            }
            Some((_, true, _)) => report.unchanged += 1,
            Some((local, false, title)) => match decide(plan, incoming_newer) {
                Outcome::Take => {
                    tx.execute(
                        "UPDATE main.Bookmark
                         SET (Title, Snippet, BlockType, BlockIdentifier) =
                             (SELECT Title, Snippet, BlockType, BlockIdentifier
                              FROM inc.Bookmark WHERE BookmarkId = ?1),
                             LocationId = ?2
                         WHERE BookmarkId = ?3",
                        params![id, location, local],
                    )?;
                    report.updated += 1;
                    if reported(plan) {
                        report.conflicts.push(conflict("Bookmark", &key, title));
                    }
                }
                Outcome::Keep => report.unchanged += 1,
                // The backup is older or as old: keep the local bookmark, but say so.
                Outcome::Conflict => report.conflicts.push(conflict("Bookmark", &key, title)),
            },
        }
    }
    Ok(())
}

/// Copy the database out of a backup into `dest` and return the manifest.
pub(crate) fn unpack_backup(src: &Path, dest: &Path) -> Result<BackupManifest> {
    let mut zip = ZipArchive::new(File::open(src)?)?;
    let manifest = read_manifest(&mut zip)?;
    let name = &manifest.user_data_backup.database_name;
    if name.is_empty() || name.contains(['/', '\\']) || name.starts_with('.') {
        return Err(Error::UnsafePath(name.clone()));
    }
    let entry = zip
        .by_name(name)
        .map_err(|_| Error::MissingEntry(name.clone()))?;
    let mut out = File::create(dest)?;
    let n = io::copy(&mut entry.take(MAX_DATABASE + 1), &mut out)?;
    if n > MAX_DATABASE {
        return Err(Error::TooLarge(format!("{name} > {MAX_DATABASE} bytes")));
    }
    out.sync_all()?;
    Ok(manifest)
}

fn read_manifest(zip: &mut ZipArchive<File>) -> Result<BackupManifest> {
    let entry = zip
        .by_name(MANIFEST)
        .map_err(|_| Error::MissingEntry(MANIFEST.into()))?;
    let mut buf = Vec::new();
    entry.take(MAX_MANIFEST + 1).read_to_end(&mut buf)?;
    if buf.len() as u64 > MAX_MANIFEST {
        return Err(Error::TooLarge(MANIFEST.into()));
    }
    Ok(serde_json::from_slice(&buf)?)
}

/// The device name in the manifest of the backup at `path`; nothing else is read.
pub fn backup_device_name(path: &Path) -> Result<String> {
    let mut zip = ZipArchive::new(File::open(path)?)?;
    Ok(read_manifest(&mut zip)?.user_data_backup.device_name)
}

/// The device name and the database hash in the manifest of the backup at
/// `path`; nothing else is read. The hash tells whether a file changed.
pub(crate) fn backup_identity(path: &Path) -> Result<(String, String)> {
    let mut zip = ZipArchive::new(File::open(path)?)?;
    let info = read_manifest(&mut zip)?.user_data_backup;
    Ok((info.device_name, info.hash))
}

/// Accept only an intact SQLite database with the user data tables.
pub(crate) fn check_database(path: &Path) -> Result<()> {
    let invalid = |reason: String| Error::InvalidDatabase {
        path: path.to_owned(),
        reason,
    };
    let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    conn.execute_batch("PRAGMA trusted_schema = OFF; PRAGMA cell_size_check = ON;")?;
    let check: String = conn.query_row("PRAGMA quick_check", [], |r| r.get(0))?;
    if check != "ok" {
        return Err(invalid(format!("integrity check: {check}")));
    }
    for table in REQUIRED_TABLES {
        let found: bool = conn.query_row(
            "SELECT count(*) > 0 FROM sqlite_master WHERE type = 'table' AND name = ?1",
            [table],
            |r| r.get(0),
        )?;
        if !found {
            return Err(invalid(format!("missing table {table}")));
        }
    }
    Ok(())
}

/// Random uppercase UUID (version 4), like the ones JW Library writes.
fn new_guid(conn: &Connection) -> Result<String> {
    let mut bytes: Vec<u8> = conn.query_row("SELECT randomblob(16)", [], |r| r.get(0))?;
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    let h = hex::encode_upper(bytes);
    Ok(format!(
        "{}-{}-{}-{}-{}",
        &h[..8],
        &h[8..12],
        &h[12..16],
        &h[16..20],
        &h[20..]
    ))
}

fn touch(tx: &Transaction<'_>) -> Result<()> {
    tx.execute(
        "UPDATE LastModified SET LastModified = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')",
        [],
    )?;
    Ok(())
}

/// Remove a note with its tags.
fn delete_note_row(tx: &Transaction<'_>, id: i64) -> Result<()> {
    tx.execute("DELETE FROM main.TagMap WHERE NoteId = ?1", [id])?;
    tx.execute("DELETE FROM main.Note WHERE NoteId = ?1", [id])?;
    Ok(())
}

fn delete_mark_row(tx: &Transaction<'_>, id: i64) -> Result<()> {
    tx.execute(
        "UPDATE Note SET UserMarkId = NULL WHERE UserMarkId = ?1",
        [id],
    )?;
    tx.execute("DELETE FROM BlockRange WHERE UserMarkId = ?1", [id])?;
    tx.execute("DELETE FROM UserMark WHERE UserMarkId = ?1", [id])?;
    Ok(())
}

/// Remove the tokens of `new` from existing highlights on the same block.
fn cut_ranges(tx: &Transaction<'_>, location: i64, new: &Range) -> Result<()> {
    let existing: Vec<(i64, i64, i64, i64)> = {
        let mut stmt = tx.prepare(
            "SELECT r.BlockRangeId, r.UserMarkId, IFNULL(r.StartToken, 0), IFNULL(r.EndToken, 0)
             FROM BlockRange r JOIN UserMark m ON m.UserMarkId = r.UserMarkId
             WHERE m.LocationId = ?1 AND r.BlockType = ?2 AND r.Identifier = ?3
               AND IFNULL(r.StartToken, 0) <= ?5 AND IFNULL(r.EndToken, 0) >= ?4",
        )?;
        let rows = stmt.query_map(
            params![location, new.block_type, new.identifier, new.start, new.end],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )?;
        rows.collect::<rusqlite::Result<_>>()?
    };
    for (range, mark, start, end) in existing {
        let left = (start < new.start).then_some((start, new.start - 1));
        let right = (end > new.end).then_some((new.end + 1, end));
        match (left, right) {
            (None, None) => {
                tx.execute("DELETE FROM BlockRange WHERE BlockRangeId = ?1", [range])?;
            }
            (Some((s, e)), None) | (None, Some((s, e))) => {
                tx.execute(
                    "UPDATE BlockRange SET StartToken = ?1, EndToken = ?2 WHERE BlockRangeId = ?3",
                    params![s, e, range],
                )?;
            }
            (Some((ls, le)), Some((rs, re))) => {
                tx.execute(
                    "UPDATE BlockRange SET StartToken = ?1, EndToken = ?2 WHERE BlockRangeId = ?3",
                    params![ls, le, range],
                )?;
                tx.execute(
                    "INSERT INTO BlockRange (BlockType, Identifier, StartToken, EndToken, UserMarkId)
                     VALUES (?1, ?2, ?3, ?4, ?5)",
                    params![new.block_type, new.identifier, rs, re, mark],
                )?;
            }
        }
        let left_over: i64 = tx.query_row(
            "SELECT count(*) FROM BlockRange WHERE UserMarkId = ?1",
            [mark],
            |r| r.get(0),
        )?;
        if left_over == 0 {
            delete_mark_row(tx, mark)?;
        }
    }
    Ok(())
}

/// Give note `note` exactly the user tags `tags`. Tags it keeps stay where
/// they are in their lists; a tag it gains puts it last in that list.
fn set_note_tags(tx: &Transaction<'_>, note: i64, tags: &[String]) -> Result<()> {
    let mut ids = Vec::new();
    for name in tags.iter().map(|t| t.trim()).filter(|t| !t.is_empty()) {
        tx.execute(
            "INSERT OR IGNORE INTO Tag (Type, Name) VALUES (?1, ?2)",
            params![TAG_USER, name],
        )?;
        let tag: i64 = tx.query_row(
            "SELECT TagId FROM Tag WHERE Type = ?1 AND Name = ?2",
            params![TAG_USER, name],
            |r| r.get(0),
        )?;
        ids.push(tag);
    }
    let keep = serde_json::to_string(&ids)?;
    tx.execute(
        "DELETE FROM TagMap WHERE NoteId = ?1
         AND TagId NOT IN (SELECT value FROM json_each(?2))",
        params![note, keep],
    )?;
    for tag in ids {
        tx.execute(
            "INSERT OR IGNORE INTO TagMap (NoteId, TagId, Position)
             VALUES (?1, ?2, (SELECT IFNULL(max(Position), -1) + 1 FROM TagMap WHERE TagId = ?2))",
            params![note, tag],
        )?;
    }
    Ok(())
}

/// Tables of a new database, laid out like JW Library's schema version 16
/// (docs/FORMAT.md) so backups made here can be restored there.
const SCHEMA: &str = "
CREATE TABLE IndependentMedia(
    IndependentMediaId INTEGER NOT NULL PRIMARY KEY,
    OriginalFilename TEXT NOT NULL,
    FilePath TEXT NOT NULL UNIQUE,
    MimeType TEXT NOT NULL,
    Hash TEXT NOT NULL);
CREATE TABLE LastModified(LastModified TEXT NOT NULL);
INSERT INTO LastModified VALUES (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'));
CREATE TABLE Location(
    LocationId INTEGER NOT NULL PRIMARY KEY,
    BookNumber INTEGER,
    ChapterNumber INTEGER,
    DocumentId INTEGER,
    Track INTEGER,
    IssueTagNumber INTEGER NOT NULL DEFAULT 0,
    KeySymbol TEXT,
    MepsLanguage INTEGER,
    Type INTEGER NOT NULL,
    Title TEXT,
    Specialty TEXT,
    Edition TEXT,
    UNIQUE(BookNumber, ChapterNumber, KeySymbol, MepsLanguage, Type));
CREATE TABLE UserMark(
    UserMarkId INTEGER NOT NULL PRIMARY KEY,
    ColorIndex INTEGER NOT NULL,
    LocationId INTEGER NOT NULL REFERENCES Location(LocationId),
    StyleIndex INTEGER NOT NULL,
    UserMarkGuid TEXT NOT NULL UNIQUE,
    Version INTEGER NOT NULL);
CREATE TABLE BlockRange(
    BlockRangeId INTEGER NOT NULL PRIMARY KEY,
    BlockType INTEGER NOT NULL CHECK (BlockType BETWEEN 1 AND 2),
    Identifier INTEGER NOT NULL,
    StartToken INTEGER,
    EndToken INTEGER,
    UserMarkId INTEGER NOT NULL REFERENCES UserMark(UserMarkId));
CREATE TABLE Bookmark(
    BookmarkId INTEGER NOT NULL PRIMARY KEY,
    LocationId INTEGER NOT NULL REFERENCES Location(LocationId),
    PublicationLocationId INTEGER NOT NULL REFERENCES Location(LocationId),
    Slot INTEGER NOT NULL,
    Title TEXT NOT NULL,
    Snippet TEXT,
    BlockType INTEGER NOT NULL DEFAULT 0,
    BlockIdentifier INTEGER,
    UNIQUE(PublicationLocationId, Slot));
CREATE TABLE InputField(
    LocationId INTEGER NOT NULL REFERENCES Location(LocationId),
    TextTag TEXT NOT NULL,
    Value TEXT NOT NULL,
    PRIMARY KEY(LocationId, TextTag));
CREATE TABLE Note(
    NoteId INTEGER NOT NULL PRIMARY KEY,
    Guid TEXT NOT NULL UNIQUE,
    UserMarkId INTEGER REFERENCES UserMark(UserMarkId),
    LocationId INTEGER REFERENCES Location(LocationId),
    Title TEXT,
    Content TEXT,
    LastModified TEXT NOT NULL DEFAULT(strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    Created TEXT NOT NULL DEFAULT(strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    BlockType INTEGER NOT NULL DEFAULT 0,
    BlockIdentifier INTEGER);
CREATE TABLE PlaylistItemAccuracy(
    PlaylistItemAccuracyId INTEGER NOT NULL PRIMARY KEY,
    Description TEXT NOT NULL UNIQUE);
INSERT INTO PlaylistItemAccuracy VALUES (1, 'Accurate'), (2, 'NeedsUserVerification');
CREATE TABLE PlaylistItem(
    PlaylistItemId INTEGER NOT NULL PRIMARY KEY,
    Label TEXT NOT NULL,
    StartTrimOffsetTicks INTEGER,
    EndTrimOffsetTicks INTEGER,
    Accuracy INTEGER NOT NULL REFERENCES PlaylistItemAccuracy(PlaylistItemAccuracyId),
    EndAction INTEGER NOT NULL,
    ThumbnailFilePath TEXT REFERENCES IndependentMedia(FilePath));
CREATE TABLE PlaylistItemIndependentMediaMap(
    PlaylistItemId INTEGER NOT NULL REFERENCES PlaylistItem(PlaylistItemId),
    IndependentMediaId INTEGER NOT NULL REFERENCES IndependentMedia(IndependentMediaId),
    DurationTicks INTEGER NOT NULL,
    PRIMARY KEY(PlaylistItemId, IndependentMediaId)) WITHOUT ROWID;
CREATE TABLE PlaylistItemLocationMap(
    PlaylistItemId INTEGER NOT NULL REFERENCES PlaylistItem(PlaylistItemId),
    LocationId INTEGER NOT NULL REFERENCES Location(LocationId),
    MajorMultimediaType INTEGER NOT NULL,
    BaseDurationTicks INTEGER,
    PRIMARY KEY(PlaylistItemId, LocationId)) WITHOUT ROWID;
CREATE TABLE PlaylistItemMarker(
    PlaylistItemMarkerId INTEGER NOT NULL PRIMARY KEY,
    PlaylistItemId INTEGER NOT NULL REFERENCES PlaylistItem(PlaylistItemId),
    Label TEXT NOT NULL,
    StartTimeTicks INTEGER NOT NULL,
    DurationTicks INTEGER NOT NULL,
    EndTransitionDurationTicks INTEGER NOT NULL,
    UNIQUE(PlaylistItemId, StartTimeTicks));
CREATE TABLE PlaylistItemMarkerBibleVerseMap(
    PlaylistItemMarkerId INTEGER NOT NULL REFERENCES PlaylistItemMarker(PlaylistItemMarkerId),
    VerseId INTEGER NOT NULL,
    PRIMARY KEY(PlaylistItemMarkerId, VerseId)) WITHOUT ROWID;
CREATE TABLE PlaylistItemMarkerParagraphMap(
    PlaylistItemMarkerId INTEGER NOT NULL REFERENCES PlaylistItemMarker(PlaylistItemMarkerId),
    MepsDocumentId INTEGER NOT NULL,
    ParagraphIndex INTEGER NOT NULL,
    MarkerIndexWithinParagraph INTEGER NOT NULL,
    PRIMARY KEY(PlaylistItemMarkerId, MepsDocumentId, ParagraphIndex, MarkerIndexWithinParagraph))
    WITHOUT ROWID;
CREATE TABLE Tag(
    TagId INTEGER NOT NULL PRIMARY KEY,
    Type INTEGER NOT NULL,
    Name TEXT NOT NULL,
    UNIQUE(Type, Name));
CREATE TABLE TagMap(
    TagMapId INTEGER NOT NULL PRIMARY KEY,
    PlaylistItemId INTEGER REFERENCES PlaylistItem(PlaylistItemId),
    LocationId INTEGER REFERENCES Location(LocationId),
    NoteId INTEGER REFERENCES Note(NoteId),
    TagId INTEGER NOT NULL REFERENCES Tag(TagId),
    Position INTEGER NOT NULL,
    UNIQUE(TagId, Position),
    UNIQUE(TagId, NoteId),
    UNIQUE(TagId, LocationId),
    UNIQUE(TagId, PlaylistItemId));
CREATE INDEX IX_BlockRange_UserMarkId ON BlockRange(UserMarkId);
CREATE INDEX IX_UserMark_LocationId ON UserMark(LocationId);
CREATE INDEX IX_Note_LocationId_BlockIdentifier ON Note(LocationId, BlockIdentifier);
CREATE INDEX IX_TagMap_NoteId_TagId_Position ON TagMap(NoteId, TagId, Position);
CREATE INDEX IX_Location_MepsLanguage_DocumentId ON Location(MepsLanguage, DocumentId);
";

#[cfg(test)]
mod tests {
    use super::*;

    fn bible(chapter: i64) -> Loc {
        Loc {
            key_symbol: "nwtsty".into(),
            meps_language: 2,
            issue_tag: 0,
            document_id: None,
            book: Some(19),
            chapter: Some(chapter),
        }
    }

    fn verse(v: i64, start: i64, end: i64) -> Range {
        Range {
            block_type: BLOCK_VERSE,
            identifier: v,
            start,
            end,
        }
    }

    fn publication_key(symbol: &str, language: i64, issue: i64) -> PublicationKey {
        PublicationKey {
            key_symbol: symbol.into(),
            meps_language: language,
            issue_tag: issue,
        }
    }

    #[test]
    fn favorites_import_add_remove_and_order() {
        let tmp = tempfile::tempdir().unwrap();
        let mut ud = UserData::open(tmp.path().join(DB_NAME)).unwrap();
        // Imported rows need neither a catalog nor downloaded publications.
        ud.conn
            .execute_batch(
                "INSERT INTO Tag VALUES (7, 0, 'Favoriten');
             INSERT INTO Location (LocationId, KeySymbol, MepsLanguage, IssueTagNumber, Type)
             VALUES (9, 'w', 0, 20260800, 1), (10, 'lff', 2, 0, 1);
             INSERT INTO TagMap (TagId, LocationId, Position) VALUES (7, 10, 8), (7, 9, 3);",
            )
            .unwrap();
        let imported = [
            publication_key("w", 0, 20260800),
            publication_key("lff", 2, 0),
        ];
        assert_eq!(ud.favorites().unwrap(), imported);
        let mark = ud.add_mark(&bible(23), None, 2, &[verse(1, 0, 3)]).unwrap();
        let note = ud
            .save_note(
                Some(&bible(23)),
                None,
                &note_input("Keep", "text", &["study"]),
            )
            .unwrap();
        // Other study data can reference a whole-publication location too.
        ud.conn
            .execute(
                "INSERT INTO Bookmark (LocationId, PublicationLocationId, Slot, Title)
             VALUES ((SELECT LocationId FROM UserMark WHERE UserMarkGuid = ?1), 9, 0, 'Keep')",
                [&mark],
            )
            .unwrap();
        let fingerprint = ud.fingerprint().unwrap();
        ud.set_favorite(&imported[0], true).unwrap();
        assert_eq!(ud.fingerprint().unwrap(), fingerprint);
        let thai = publication_key("ขก", 237, 0);
        ud.set_favorite(&thai, true).unwrap();
        ud.set_favorite(&thai, true).unwrap();
        assert_eq!(
            ud.favorites().unwrap(),
            [imported[0].clone(), imported[1].clone(), thai.clone()]
        );
        // The existing localized type-0 tag is reused.
        assert_eq!(
            ud.conn
                .query_row("SELECT count(*) FROM Tag WHERE Type = 0", [], |r| r
                    .get::<_, i64>(0))
                .unwrap(),
            1
        );
        ud.set_favorite(&imported[0], false).unwrap();
        let fingerprint = ud.fingerprint().unwrap();
        ud.set_favorite(&imported[0], false).unwrap();
        assert_eq!(ud.fingerprint().unwrap(), fingerprint);
        assert_eq!(ud.favorites().unwrap(), [imported[1].clone(), thai]);
        assert_eq!(ud.marks(&bible(23)).unwrap()[0].guid, mark);
        assert_eq!(ud.all_notes(None).unwrap()[0].guid, note);
        assert_eq!(ud.bookmarks().unwrap().len(), 1);
        assert_eq!(
            ud.conn
                .query_row(
                    "SELECT count(*) FROM Location WHERE LocationId = 9",
                    [],
                    |r| r.get::<_, i64>(0)
                )
                .unwrap(),
            1
        );
        ud.set_favorite(&imported[0], true).unwrap();
        assert_eq!(ud.favorites().unwrap().last(), Some(&imported[0]));
        assert_eq!(
            ud.conn
                .query_row(
                    "SELECT count(*) FROM Location WHERE KeySymbol = 'w'",
                    [],
                    |r| r.get::<_, i64>(0)
                )
                .unwrap(),
            1
        );
    }

    #[test]
    fn favorites_respect_jw_library_location_check_after_restore() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join(DB_NAME);
        let conn = Connection::open(&path).unwrap();
        let schema = SCHEMA.replace(
            "UNIQUE(BookNumber, ChapterNumber, KeySymbol, MepsLanguage, Type)",
            "CHECK (
                (Type = 0 AND (
                    (DocumentId IS NOT NULL AND DocumentId != 0)
                    OR (Track IS NOT NULL AND (
                        (KeySymbol IS NOT NULL AND length(KeySymbol) > 0)
                        OR (DocumentId IS NOT NULL AND DocumentId != 0)))
                    OR (BookNumber IS NOT NULL AND BookNumber != 0
                        AND KeySymbol IS NOT NULL AND length(KeySymbol) > 0
                        AND (ChapterNumber IS NULL OR ChapterNumber = 0))
                    OR (ChapterNumber IS NOT NULL AND ChapterNumber != 0
                        AND BookNumber IS NOT NULL AND BookNumber != 0
                        AND KeySymbol IS NOT NULL AND length(KeySymbol) > 0)))
                OR Type != 0),
             UNIQUE(BookNumber, ChapterNumber, KeySymbol, MepsLanguage, Type)",
        );
        conn.execute_batch(&schema).unwrap();
        conn.pragma_update(None, "user_version", SCHEMA_VERSION)
            .unwrap();
        drop(conn);
        let source = UserData::open(&path).unwrap();
        let backup = backup_of(&source, tmp.path(), "jw-library.jwlibrary");
        let mut ud = UserData::open(tmp.path().join("restored.db")).unwrap();
        ud.restore_backup(&backup).unwrap();
        assert!(
            ud.conn
                .execute(
                    "INSERT INTO Location (KeySymbol, MepsLanguage, Type) VALUES ('bad', 2, 0)",
                    []
                )
                .is_err()
        );
        let keys = [
            publication_key("lff", 2, 0),
            publication_key("w", 0, 20260800),
            publication_key("nwtsty", 2, 0),
        ];
        for key in &keys {
            ud.set_favorite(key, true).unwrap();
            ud.set_favorite(key, true).unwrap();
        }
        assert_eq!(ud.favorites().unwrap(), keys);
        assert_eq!(
            ud.conn
                .query_row("SELECT count(*) FROM Location WHERE Type = 1", [], |r| r
                    .get::<_, i64>(0))
                .unwrap(),
            3
        );
        let page = Loc {
            key_symbol: "lff".into(),
            meps_language: 2,
            issue_tag: 0,
            document_id: Some(1),
            book: None,
            chapter: None,
        };
        let note = ud
            .save_note(Some(&page), None, &note_input("Keep", "text", &[]))
            .unwrap();
        ud.set_favorite(&keys[0], false).unwrap();
        let backup = backup_of(&ud, tmp.path(), "roundtrip.jwlibrary");
        let mut restored = UserData::open(tmp.path().join("roundtrip.db")).unwrap();
        restored.restore_backup(&backup).unwrap();
        assert_eq!(restored.favorites().unwrap(), keys[1..]);
        assert_eq!(restored.all_notes(None).unwrap()[0].guid, note);
    }

    #[test]
    fn favorites_validate_and_roll_back_database_failures() {
        let tmp = tempfile::tempdir().unwrap();
        let mut ud = UserData::open(tmp.path().join(DB_NAME)).unwrap();
        for key in [
            publication_key("", 2, 0),
            publication_key("a b", 2, 0),
            publication_key("x", -1, 0),
            publication_key("x", 2, -1),
        ] {
            assert!(ud.set_favorite(&key, true).is_err());
            assert!(ud.set_favorite(&key, false).is_err());
        }
        let key = publication_key("lff", 2, 0);
        let before = ud.fingerprint().unwrap();
        ud.conn.execute_batch(
            "CREATE TRIGGER fail_add BEFORE INSERT ON TagMap BEGIN SELECT RAISE(ABORT, 'add failed'); END;"
        ).unwrap();
        assert!(
            ud.set_favorite(&key, true)
                .unwrap_err()
                .to_string()
                .contains("add failed")
        );
        assert_eq!(ud.fingerprint().unwrap(), before);
        ud.conn.execute_batch("DROP TRIGGER fail_add;").unwrap();
        ud.set_favorite(&key, true).unwrap();
        let before = ud.fingerprint().unwrap();
        ud.conn.execute_batch(
            "CREATE TRIGGER fail_remove BEFORE DELETE ON TagMap BEGIN SELECT RAISE(ABORT, 'remove failed'); END;"
        ).unwrap();
        assert!(
            ud.set_favorite(&key, false)
                .unwrap_err()
                .to_string()
                .contains("remove failed")
        );
        assert_eq!(ud.fingerprint().unwrap(), before);
        assert_eq!(ud.favorites().unwrap(), [key]);
        ud.conn.execute_batch("DROP TABLE TagMap;").unwrap();
        assert!(ud.favorites().is_err());
    }

    #[test]
    fn favorites_backup_restore_and_merge_keep_study_data() {
        let tmp = tempfile::tempdir().unwrap();
        let mut source = UserData::open(tmp.path().join("source").join(DB_NAME)).unwrap();
        let keys = [
            publication_key("w", 0, 20260800),
            publication_key("nwtsty", 2, 0),
        ];
        for key in &keys {
            source.set_favorite(key, true).unwrap();
        }
        source
            .add_mark(&bible(23), None, 3, &[verse(1, 0, 3)])
            .unwrap();
        source
            .save_note(
                Some(&bible(23)),
                None,
                &note_input("Source", "keep", &["study"]),
            )
            .unwrap();
        source
            .save_answer(&bible(23), None, "answer", "keep")
            .unwrap();
        let file = tmp.path().join("backup.jwlibrary");
        source.export_backup(&file).unwrap();
        let mut restored = UserData::open(tmp.path().join("restored").join(DB_NAME)).unwrap();
        restored.restore_backup(&file).unwrap();
        assert_eq!(restored.favorites().unwrap(), keys);
        assert_eq!(
            restored.fingerprint().unwrap(),
            source.fingerprint().unwrap()
        );
        let local = publication_key("lff", 2, 0);
        let mut merged = UserData::open(tmp.path().join("merged").join(DB_NAME)).unwrap();
        merged.set_favorite(&local, true).unwrap();
        merged
            .save_note(Some(&bible(1)), None, &note_input("Local", "keep", &[]))
            .unwrap();
        merged.merge_backup(&file).unwrap();
        assert_eq!(
            merged.favorites().unwrap(),
            [local.clone(), keys[0].clone(), keys[1].clone()]
        );
        merged.merge_backup(&file).unwrap();
        assert_eq!(
            merged.favorites().unwrap(),
            [local, keys[0].clone(), keys[1].clone()]
        );
        assert_eq!(merged.all_notes(None).unwrap().len(), 2);
        assert_eq!(merged.marks(&bible(23)).unwrap().len(), 1);
        assert_eq!(merged.answers(&bible(23)).unwrap()["answer"], "keep");
    }

    #[test]
    fn marks_round_trip_and_split() {
        let tmp = tempfile::tempdir().unwrap();
        let mut ud = UserData::open(tmp.path().join(DB_NAME)).unwrap();
        let a = ud
            .add_mark(&bible(23), Some("Psalm 23"), 3, &[verse(4, 0, 31)])
            .unwrap();
        assert_eq!(a.len(), 36);
        assert_eq!(&a[14..15], "4");
        // A new color in the middle splits the old highlight.
        let b = ud
            .add_mark(&bible(23), None, 1, &[verse(4, 10, 12)])
            .unwrap();
        let marks = ud.marks(&bible(23)).unwrap();
        assert_eq!(marks.len(), 2);
        assert_eq!(marks[0].ranges, vec![verse(4, 0, 9), verse(4, 13, 31)]);
        assert_eq!(marks[1].guid, b);
        assert!(ud.marks(&bible(24)).unwrap().is_empty());
        // Covering it completely removes the old one.
        ud.add_mark(&bible(23), None, 2, &[verse(4, 0, 40)])
            .unwrap();
        let marks = ud.marks(&bible(23)).unwrap();
        assert_eq!(marks.len(), 1);
        assert_eq!(marks[0].color, 2);
        ud.set_mark_color(&marks[0].guid, 5).unwrap();
        assert_eq!(ud.marks(&bible(23)).unwrap()[0].color, 5);
        ud.delete_mark(&marks[0].guid).unwrap();
        assert!(ud.marks(&bible(23)).unwrap().is_empty());
    }

    #[test]
    fn answers_round_trip() {
        let tmp = tempfile::tempdir().unwrap();
        let mut ud = UserData::open(tmp.path().join(DB_NAME)).unwrap();
        let loc = Loc {
            key_symbol: "w".into(),
            meps_language: 2,
            issue_tag: 20260800,
            document_id: Some(2026520),
            book: None,
            chapter: None,
        };
        assert!(ud.answers(&loc).unwrap().is_empty());
        ud.save_answer(&loc, Some("Title"), "tt91", "first")
            .unwrap();
        ud.save_answer(&loc, None, "tt91", "second").unwrap();
        ud.save_answer(&loc, None, "tt95", "other").unwrap();
        let answers = ud.answers(&loc).unwrap();
        assert_eq!(answers["tt91"], "second");
        assert_eq!(answers.len(), 2);
        ud.save_answer(&loc, None, "tt95", "").unwrap();
        assert_eq!(ud.answers(&loc).unwrap().len(), 1);
    }

    #[test]
    fn answers_of_a_location_without_language() {
        let tmp = tempfile::tempdir().unwrap();
        let mut ud = UserData::open(tmp.path().join(DB_NAME)).unwrap();
        // JW Library stores answer fields on locations without a language.
        ud.conn
            .execute_batch(
                "INSERT INTO Location (LocationId, DocumentId, IssueTagNumber, KeySymbol, Type)
                 VALUES (387, 2026520, 20260800, 'w', 0);
                 INSERT INTO InputField VALUES (387, 'tt91', 'from backup');",
            )
            .unwrap();
        let loc = Loc {
            key_symbol: "w".into(),
            meps_language: 2,
            issue_tag: 20260800,
            document_id: Some(2026520),
            book: None,
            chapter: None,
        };
        assert_eq!(ud.answers(&loc).unwrap()["tt91"], "from backup");
        ud.save_answer(&loc, None, "tt91", "edited").unwrap();
        assert_eq!(ud.answers(&loc).unwrap()["tt91"], "edited");
        let rows: i64 = ud
            .conn
            .query_row("SELECT COUNT(*) FROM Location", [], |r| r.get(0))
            .unwrap();
        assert_eq!(rows, 1);
    }
    #[test]
    fn notes_and_tags() {
        let tmp = tempfile::tempdir().unwrap();
        let mut ud = UserData::open(tmp.path().join(DB_NAME)).unwrap();
        let mark = ud.add_mark(&bible(23), None, 3, &[verse(1, 0, 3)]).unwrap();
        let guid = ud
            .save_note(
                Some(&bible(23)),
                Some("Psalm 23"),
                &NoteInput {
                    title: "Shepherd".into(),
                    content: "Text".into(),
                    block_type: BLOCK_VERSE,
                    block_identifier: Some(1),
                    mark_guid: Some(mark.clone()),
                    tags: vec!["Comfort".into(), " ".into()],
                    ..Default::default()
                },
            )
            .unwrap();
        let notes = ud.notes(&bible(23)).unwrap();
        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0].mark_guid.as_deref(), Some(mark.as_str()));
        assert_eq!(notes[0].color, Some(3));
        assert_eq!(notes[0].tags, ["Comfort"]);
        assert_eq!(
            notes[0].location.as_ref().unwrap().title.as_deref(),
            Some("Psalm 23")
        );

        ud.save_note(
            None,
            None,
            &NoteInput {
                guid: Some(guid.clone()),
                title: "Shepherd".into(),
                content: "Changed".into(),
                tags: vec!["Prayer".into()],
                ..Default::default()
            },
        )
        .unwrap();
        let tags = ud.tags().unwrap();
        assert_eq!(
            tags.iter()
                .map(|t| (t.name.as_str(), t.notes))
                .collect::<Vec<_>>(),
            [("Comfort", 0), ("Prayer", 1)]
        );
        let prayer = tags[1].id;
        assert_eq!(ud.all_notes(Some(prayer)).unwrap()[0].content, "Changed");
        assert_eq!(ud.all_notes(Some(tags[0].id)).unwrap().len(), 0);

        // Deleting the highlight keeps the note.
        ud.delete_mark(&mark).unwrap();
        assert_eq!(ud.notes(&bible(23)).unwrap()[0].mark_guid, None);
        ud.delete_note(&guid).unwrap();
        assert!(ud.all_notes(None).unwrap().is_empty());
    }

    #[test]
    fn saving_a_note_again_keeps_its_tag_order() {
        let tmp = tempfile::tempdir().unwrap();
        let mut ud = UserData::open(tmp.path().join(DB_NAME)).unwrap();
        let first = ud
            .save_note(
                Some(&bible(1)),
                None,
                &note_input("One", "a", &["joy", "study"]),
            )
            .unwrap();
        let second = ud
            .save_note(Some(&bible(1)), None, &note_input("Two", "b", &["study"]))
            .unwrap();
        // (tag, note guid) by position within each tag.
        let order = |ud: &UserData| -> Vec<(String, String)> {
            let mut stmt = ud
                .conn
                .prepare(
                    "SELECT t.Name, n.Guid FROM TagMap m JOIN Tag t ON t.TagId = m.TagId
                     JOIN Note n ON n.NoteId = m.NoteId ORDER BY t.Name, m.Position",
                )
                .unwrap();
            stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
                .unwrap()
                .collect::<rusqlite::Result<_>>()
                .unwrap()
        };
        let before = order(&ud);
        edit_note(&mut ud, &first, "One", "edited", &["joy", "study"]);
        assert_eq!(order(&ud), before);

        // A removed tag goes, an added one comes last in its list.
        edit_note(&mut ud, &first, "One", "edited", &["study", "zeal"]);
        let s = |t: &str, g: &str| (t.to_string(), g.to_string());
        assert_eq!(
            order(&ud),
            [s("study", &first), s("study", &second), s("zeal", &first)]
        );
        edit_note(&mut ud, &second, "Two", "b", &["joy", "zeal"]);
        assert_eq!(
            order(&ud),
            [
                s("joy", &second),
                s("study", &first),
                s("zeal", &first),
                s("zeal", &second)
            ]
        );
    }

    #[test]
    fn backup_round_trip() {
        let tmp = tempfile::tempdir().unwrap();
        let mut ud = UserData::open(tmp.path().join("a").join(DB_NAME)).unwrap();
        ud.add_mark(&bible(23), None, 6, &[verse(1, 0, 3)]).unwrap();
        let backup = tmp.path().join("b.jwlibrary");
        ud.export_backup(&backup).unwrap();

        let mut zip = ZipArchive::new(File::open(&backup).unwrap()).unwrap();
        let manifest: BackupManifest =
            serde_json::from_reader(zip.by_name(MANIFEST).unwrap()).unwrap();
        assert_eq!(manifest.user_data_backup.database_name, DB_NAME);
        assert_eq!(manifest.user_data_backup.schema_version, SCHEMA_VERSION);
        let mut db = Vec::new();
        zip.by_name(DB_NAME).unwrap().read_to_end(&mut db).unwrap();
        assert_eq!(
            manifest.user_data_backup.hash,
            hex::encode(Sha256::digest(&db))
        );
        assert!(zip.by_name(THUMBNAIL).is_ok());

        let mut other = UserData::open(tmp.path().join("c").join(DB_NAME)).unwrap();
        other
            .add_mark(&bible(1), None, 1, &[verse(1, 0, 0)])
            .unwrap();
        let summary = other.restore_backup(&backup).unwrap();
        assert_eq!(summary.marks, 1);
        assert_eq!(summary.device.as_deref(), Some(DEVICE_NAME));
        assert_eq!(other.marks(&bible(23)).unwrap()[0].color, 6);
        assert!(other.marks(&bible(1)).unwrap().is_empty());
        assert!(
            tmp.path()
                .join("c")
                .join("userData.db.before-restore")
                .is_file()
        );
    }

    #[test]
    fn rejects_bad_backups() {
        let tmp = tempfile::tempdir().unwrap();
        let mut ud = UserData::open(tmp.path().join(DB_NAME)).unwrap();
        ud.add_mark(&bible(23), None, 6, &[verse(1, 0, 3)]).unwrap();
        let write = |name: &str, manifest: &str, db: &[u8]| {
            let path = tmp.path().join(name);
            let mut zip = zip::ZipWriter::new(File::create(&path).unwrap());
            zip.start_file(MANIFEST, SimpleFileOptions::default())
                .unwrap();
            zip.write_all(manifest.as_bytes()).unwrap();
            zip.start_file("userData.db", SimpleFileOptions::default())
                .unwrap();
            zip.write_all(db).unwrap();
            zip.finish().unwrap();
            path
        };
        let manifest = |db: &str| {
            format!(
                r#"{{"creationDate":"x","version":1,"name":"x","type":0,"userDataBackup":
                   {{"lastModifiedDate":"x","hash":"x","databaseName":"{db}","deviceName":"x","schemaVersion":16}}}}"#
            )
        };
        for (name, m, db) in [
            ("garbage", manifest("userData.db"), b"not sqlite".as_slice()),
            ("traversal", manifest("../userData.db"), b"".as_slice()),
            ("missing", manifest("other.db"), b"".as_slice()),
            ("nomanifest", "{}".to_owned(), b"".as_slice()),
        ] {
            let path = write(name, &m, db);
            assert!(ud.restore_backup(&path).is_err(), "{name}");
        }
        // A failed restore leaves the data alone.
        assert_eq!(ud.summary().unwrap().marks, 1);
    }

    fn backup_of(ud: &UserData, dir: &Path, name: &str) -> PathBuf {
        let p = dir.join(name);
        ud.export_backup(&p).unwrap();
        p
    }

    fn set_db_time(ud: &UserData, ts: &str) {
        ud.conn
            .execute("UPDATE LastModified SET LastModified = ?1", [ts])
            .unwrap();
    }

    fn db_time(ud: &UserData) -> String {
        ud.conn
            .query_row("SELECT LastModified FROM LastModified", [], |r| r.get(0))
            .unwrap()
    }

    fn count(ud: &UserData, table: &str) -> i64 {
        ud.conn
            .query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))
            .unwrap()
    }

    fn watchtower() -> Loc {
        Loc {
            key_symbol: "w".into(),
            meps_language: 2,
            issue_tag: 20260800,
            document_id: Some(2026520),
            book: None,
            chapter: None,
        }
    }

    #[test]
    fn merge_keeps_disjoint_marks_from_both_sides() {
        let tmp = tempfile::tempdir().unwrap();
        let mut local = UserData::open(tmp.path().join("l").join(DB_NAME)).unwrap();
        let mut remote = UserData::open(tmp.path().join("r").join(DB_NAME)).unwrap();
        local
            .add_mark(&bible(1), None, 1, &[verse(1, 0, 3)])
            .unwrap();
        remote
            .add_mark(&bible(2), None, 2, &[verse(5, 0, 3)])
            .unwrap();
        let b = backup_of(&remote, tmp.path(), "r.jwlibrary");
        let r = local.merge_backup(&b).unwrap();
        assert_eq!((r.added, r.updated, r.unchanged), (1, 0, 0));
        assert!(r.conflicts.is_empty());
        assert_eq!(r.device.as_deref(), Some(DEVICE_NAME));
        assert_eq!(local.marks(&bible(1)).unwrap().len(), 1);
        assert_eq!(local.marks(&bible(2)).unwrap()[0].color, 2);
        assert_eq!(local.marks(&bible(2)).unwrap()[0].ranges, [verse(5, 0, 3)]);
        assert!(local.path().with_extension("db.before-restore").is_file());
    }

    #[cfg(unix)]
    #[test]
    fn a_failed_safety_copy_keeps_the_older_one() {
        use std::os::unix::fs::PermissionsExt;
        let tmp = tempfile::tempdir().unwrap();
        let local = UserData::open(tmp.path().join("l").join(DB_NAME)).unwrap();
        let dir = local.path().parent().unwrap().to_owned();
        let safety = local.path().with_extension("db.before-restore");
        fs::write(&safety, b"old copy").unwrap();
        // VACUUM INTO cannot create its file in a read-only directory.
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o555)).unwrap();
        let failed = local.write_safety_copy().is_err();
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o755)).unwrap();
        // Root ignores the permissions, so there is nothing to check then.
        if failed {
            assert_eq!(fs::read(&safety).unwrap(), b"old copy");
            assert!(
                !local
                    .path()
                    .with_extension("db.before-restore.tmp")
                    .exists()
            );
        }
    }

    #[test]
    fn merge_replaces_an_older_safety_copy() {
        let tmp = tempfile::tempdir().unwrap();
        let mut local = UserData::open(tmp.path().join("l").join(DB_NAME)).unwrap();
        let remote = UserData::open(tmp.path().join("r").join(DB_NAME)).unwrap();
        let safety = local.path().with_extension("db.before-restore");
        fs::write(&safety, b"old copy").unwrap();
        local
            .merge_backup(&backup_of(&remote, tmp.path(), "r.jwlibrary"))
            .unwrap();
        let copy = UserData::open(&safety).unwrap();
        assert_eq!(count(&copy, "UserMark"), 0);
    }

    #[test]
    fn merge_same_location_with_different_ids_makes_no_duplicate_location() {
        let tmp = tempfile::tempdir().unwrap();
        let mut local = UserData::open(tmp.path().join("l").join(DB_NAME)).unwrap();
        let mut remote = UserData::open(tmp.path().join("r").join(DB_NAME)).unwrap();
        local
            .add_mark(&bible(9), None, 1, &[verse(1, 0, 3)])
            .unwrap(); // LocationId 1
        remote
            .add_mark(&bible(1), None, 1, &[verse(1, 0, 3)])
            .unwrap(); // LocationId 1 = other place
        remote
            .add_mark(&bible(9), None, 3, &[verse(7, 0, 3)])
            .unwrap(); // chapter 9 is id 2 here
        let b = backup_of(&remote, tmp.path(), "r.jwlibrary");
        local.merge_backup(&b).unwrap();
        assert_eq!(count(&local, "Location"), 2);
        assert_eq!(local.marks(&bible(9)).unwrap().len(), 2);
        assert_eq!(local.marks(&bible(1)).unwrap().len(), 1);
    }

    #[test]
    fn merge_location_without_language_stays_apart() {
        let tmp = tempfile::tempdir().unwrap();
        let mut local = UserData::open(tmp.path().join("l").join(DB_NAME)).unwrap();
        let remote = UserData::open(tmp.path().join("r").join(DB_NAME)).unwrap();
        local
            .save_answer(&watchtower(), None, "tt1", "local")
            .unwrap();
        // JW Library writes answer locations without a language: NULL must
        // not match 2, and NULL must match NULL.
        remote
            .conn
            .execute_batch(
                "INSERT INTO Location (LocationId, DocumentId, IssueTagNumber, KeySymbol, Type)
                 VALUES (7, 2026520, 20260800, 'w', 0);
                 INSERT INTO InputField VALUES (7, 'tt2', 'remote');",
            )
            .unwrap();
        let b = backup_of(&remote, tmp.path(), "r.jwlibrary");
        let r = local.merge_backup(&b).unwrap();
        assert_eq!(r.added, 1);
        assert_eq!(count(&local, "Location"), 2);
        let again = local.merge_backup(&b).unwrap();
        assert_eq!((again.added, again.unchanged), (0, 1));
        assert_eq!(count(&local, "Location"), 2);
        let answers = local.answers(&watchtower()).unwrap();
        assert_eq!(answers["tt1"], "local");
        assert_eq!(answers["tt2"], "remote");
    }

    #[test]
    fn merge_same_guid_newer_database_wins_marks() {
        let tmp = tempfile::tempdir().unwrap();
        let mut local = UserData::open(tmp.path().join("l").join(DB_NAME)).unwrap();
        let guid = local
            .add_mark(&bible(1), None, 1, &[verse(1, 0, 3)])
            .unwrap();
        let remote_path = tmp.path().join("r").join(DB_NAME);
        let mut remote = UserData::open(&remote_path).unwrap();
        // The remote starts as a copy of local, then recolors the mark.
        remote
            .restore_backup(&backup_of(&local, tmp.path(), "seed.jwlibrary"))
            .unwrap();
        remote.set_mark_color(&guid, 5).unwrap();
        set_db_time(&remote, "2030-01-01T00:00:00Z");
        set_db_time(&local, "2020-01-01T00:00:00Z");
        let r = local
            .merge_backup(&backup_of(&remote, tmp.path(), "r.jwlibrary"))
            .unwrap();
        assert_eq!((r.added, r.updated), (0, 1));
        assert_eq!(local.marks(&bible(1)).unwrap()[0].color, 5);
        // Reverse the clocks: the local color stays.
        set_db_time(&local, "2040-01-01T00:00:00Z");
        local.set_mark_color(&guid, 2).unwrap();
        set_db_time(&local, "2040-01-01T00:00:00Z");
        let r = local
            .merge_backup(&backup_of(&remote, tmp.path(), "r2.jwlibrary"))
            .unwrap();
        // The older backup's color is not taken, but it is reported.
        assert_eq!((r.added, r.updated, r.unchanged), (0, 0, 0));
        assert_eq!(r.conflicts.len(), 1);
        assert_eq!(r.conflicts[0].table, "UserMark");
        assert_eq!(r.conflicts[0].guid, guid);
        assert_eq!(r.conflicts[0].summary, "nwtsty 19:1, color 5 in the backup");
        assert_eq!(local.marks(&bible(1)).unwrap()[0].color, 2);
    }

    #[test]
    fn merge_same_guid_newer_database_replaces_ranges() {
        let tmp = tempfile::tempdir().unwrap();
        let mut local = UserData::open(tmp.path().join("l").join(DB_NAME)).unwrap();
        let guid = local
            .add_mark(&bible(1), None, 1, &[verse(1, 0, 3)])
            .unwrap();
        let mut remote = UserData::open(tmp.path().join("r").join(DB_NAME)).unwrap();
        remote
            .restore_backup(&backup_of(&local, tmp.path(), "seed.jwlibrary"))
            .unwrap();
        remote
            .conn
            .execute_batch(
                "UPDATE BlockRange SET EndToken = 8;
                 INSERT INTO BlockRange (BlockType, Identifier, StartToken, EndToken, UserMarkId)
                 VALUES (2, 2, 0, 4, 1);",
            )
            .unwrap();
        set_db_time(&remote, "2030-01-01T00:00:00+00:00");
        set_db_time(&local, "2029-12-31T23:59:59Z");
        let r = local
            .merge_backup(&backup_of(&remote, tmp.path(), "r.jwlibrary"))
            .unwrap();
        assert_eq!(r.updated, 1);
        let marks = local.marks(&bible(1)).unwrap();
        assert_eq!(marks.len(), 1);
        assert_eq!(marks[0].guid, guid);
        assert_eq!(marks[0].ranges, [verse(1, 0, 8), verse(2, 0, 4)]);
        assert_eq!(count(&local, "BlockRange"), 2);
    }

    #[test]
    fn merge_equal_clock_different_color_is_a_conflict_and_keeps_local() {
        let tmp = tempfile::tempdir().unwrap();
        let mut local = UserData::open(tmp.path().join("l").join(DB_NAME)).unwrap();
        let guid = local
            .add_mark(&bible(1), Some("Genesis 1"), 1, &[verse(1, 0, 3)])
            .unwrap();
        let mut remote = UserData::open(tmp.path().join("r").join(DB_NAME)).unwrap();
        remote
            .restore_backup(&backup_of(&local, tmp.path(), "seed.jwlibrary"))
            .unwrap();
        remote.set_mark_color(&guid, 5).unwrap();
        local.set_mark_color(&guid, 2).unwrap();
        // The same instant, written two ways.
        set_db_time(&remote, "2030-01-01T00:00:00Z");
        set_db_time(&local, "2030-01-01T00:00:00+00:00");
        let r = local
            .merge_backup(&backup_of(&remote, tmp.path(), "r.jwlibrary"))
            .unwrap();
        assert_eq!((r.added, r.updated, r.unchanged), (0, 0, 0));
        assert_eq!(r.conflicts.len(), 1);
        assert_eq!(r.conflicts[0].table, "UserMark");
        assert_eq!(r.conflicts[0].guid, guid);
        assert_eq!(r.conflicts[0].summary, "Genesis 1, color 5 in the backup");
        assert_eq!(local.marks(&bible(1)).unwrap()[0].color, 2);
        // Nothing changed, so the local clock stays.
        assert_eq!(db_time(&local), "2030-01-01T00:00:00+00:00");
    }

    #[test]
    fn merge_overlapping_highlights_follow_the_newer_database() {
        for remote_newer in [true, false] {
            let tmp = tempfile::tempdir().unwrap();
            let mut local = UserData::open(tmp.path().join("l").join(DB_NAME)).unwrap();
            let mut remote = UserData::open(tmp.path().join("r").join(DB_NAME)).unwrap();
            let local_guid = local
                .add_mark(&bible(1), None, 1, &[verse(1, 0, 9)])
                .unwrap();
            let remote_guid = remote
                .add_mark(&bible(1), None, 2, &[verse(1, 5, 12)])
                .unwrap();
            let (old, new) = ("2020-01-01T00:00:00Z", "2030-01-01T00:00:00Z");
            set_db_time(&local, if remote_newer { old } else { new });
            set_db_time(&remote, if remote_newer { new } else { old });
            let r = local
                .merge_backup(&backup_of(&remote, tmp.path(), "r.jwlibrary"))
                .unwrap();
            let marks = local.marks(&bible(1)).unwrap();
            if remote_newer {
                assert_eq!(r.added, 1);
                assert!(r.conflicts.is_empty());
                assert_eq!(marks.len(), 2);
                assert_eq!(
                    (marks[0].guid.as_str(), marks[0].color),
                    (local_guid.as_str(), 1)
                );
                assert_eq!(marks[0].ranges, [verse(1, 0, 4)]);
                assert_eq!(
                    (marks[1].guid.as_str(), marks[1].color),
                    (remote_guid.as_str(), 2)
                );
                assert_eq!(marks[1].ranges, [verse(1, 5, 12)]);
            } else {
                assert_eq!(r.added, 0);
                assert_eq!(r.conflicts.len(), 1);
                assert_eq!(r.conflicts[0].table, "UserMark");
                assert_eq!(r.conflicts[0].guid, remote_guid);
                assert_eq!(marks.len(), 1);
                assert_eq!(marks[0].guid, local_guid);
                assert_eq!(marks[0].ranges, [verse(1, 0, 9)]);
            }
        }
    }

    #[test]
    fn merge_input_fields_follow_the_newer_database() {
        let (old, new) = ("2020-01-01T00:00:00Z", "2030-01-01T00:00:00Z");
        for (local_time, remote_time, expected) in [
            (old, new, "remote"),
            (new, old, "local"),
            (new, new, "local"),
        ] {
            let tmp = tempfile::tempdir().unwrap();
            let mut local = UserData::open(tmp.path().join("l").join(DB_NAME)).unwrap();
            let mut remote = UserData::open(tmp.path().join("r").join(DB_NAME)).unwrap();
            local
                .save_answer(&watchtower(), None, "tt1", "local")
                .unwrap();
            remote
                .save_answer(&watchtower(), None, "tt1", "remote")
                .unwrap();
            remote
                .save_answer(&watchtower(), None, "tt2", "new")
                .unwrap();
            set_db_time(&local, local_time);
            set_db_time(&remote, remote_time);
            let r = local
                .merge_backup(&backup_of(&remote, tmp.path(), "r.jwlibrary"))
                .unwrap();
            let answers = local.answers(&watchtower()).unwrap();
            assert_eq!(answers["tt1"], expected);
            assert_eq!(answers["tt2"], "new");
            assert_eq!(r.added, 1);
            assert_eq!(r.updated, i64::from(expected == "remote"));
            if expected == "local" {
                // Kept, whether the backup is older or as old: reported.
                assert_eq!(r.conflicts.len(), 1);
                assert_eq!(r.conflicts[0].table, "InputField");
                // Local LocationId and TextTag.
                assert_eq!(r.conflicts[0].guid, "1:tt1");
                assert_eq!(r.conflicts[0].summary, "w 2026520, field tt1");
            } else {
                assert!(r.conflicts.is_empty());
            }
            assert_eq!(count(&local, "Location"), 1);
        }
    }

    #[test]
    fn merge_is_idempotent() {
        let tmp = tempfile::tempdir().unwrap();
        let mut local = UserData::open(tmp.path().join("l").join(DB_NAME)).unwrap();
        let mut remote = UserData::open(tmp.path().join("r").join(DB_NAME)).unwrap();
        local
            .add_mark(&bible(1), None, 1, &[verse(1, 0, 3)])
            .unwrap();
        local
            .save_answer(&watchtower(), None, "tt1", "local")
            .unwrap();
        remote
            .add_mark(&bible(1), None, 3, &[verse(5, 0, 3)])
            .unwrap();
        remote
            .add_mark(&bible(2), None, 4, &[verse(1, 2, 6), verse(2, 0, 1)])
            .unwrap();
        remote
            .save_answer(&watchtower(), None, "tt2", "remote")
            .unwrap();
        let b = backup_of(&remote, tmp.path(), "r.jwlibrary");
        let counts = |ud: &UserData| {
            ["UserMark", "BlockRange", "Location", "InputField"].map(|t| count(ud, t))
        };
        let first = local.merge_backup(&b).unwrap();
        assert_eq!((first.added, first.updated, first.unchanged), (3, 0, 0));
        let after_first = counts(&local);
        assert_eq!(after_first, [3, 4, 3, 2]);
        set_db_time(&local, "2020-01-01T00:00:00Z");
        let second = local.merge_backup(&b).unwrap();
        assert_eq!((second.added, second.updated, second.unchanged), (0, 0, 3));
        assert!(second.conflicts.is_empty());
        assert_eq!(counts(&local), after_first);
        // A merge that changes nothing leaves the clock alone.
        assert_eq!(db_time(&local), "2020-01-01T00:00:00Z");
    }

    #[test]
    fn merge_rejects_a_bad_backup_and_changes_nothing() {
        let tmp = tempfile::tempdir().unwrap();
        let mut local = UserData::open(tmp.path().join("l").join(DB_NAME)).unwrap();
        local
            .add_mark(&bible(1), None, 4, &[verse(1, 0, 3)])
            .unwrap();
        let counts = |ud: &UserData| ["UserMark", "BlockRange", "Location"].map(|t| count(ud, t));
        let before = counts(&local);
        let safety = local.path().with_extension("db.before-restore");
        let attached = |ud: &UserData| -> Vec<String> {
            let mut stmt = ud
                .conn
                .prepare("SELECT name FROM pragma_database_list")
                .unwrap();
            stmt.query_map([], |r| r.get(0))
                .unwrap()
                .collect::<rusqlite::Result<_>>()
                .unwrap()
        };

        // Not a zip at all.
        let garbage = tmp.path().join("x.jwlibrary");
        fs::write(&garbage, b"not a backup").unwrap();
        assert!(local.merge_backup(&garbage).is_err());

        // A database without the Note table.
        let db = tmp.path().join("nonote.db");
        {
            let conn = Connection::open(&db).unwrap();
            conn.execute_batch(SCHEMA).unwrap();
            conn.execute_batch("PRAGMA foreign_keys = OFF; DROP TABLE Note;")
                .unwrap();
        }
        let nonote = tmp.path().join("nonote.jwlibrary");
        {
            let mut zip = zip::ZipWriter::new(File::create(&nonote).unwrap());
            zip.start_file(MANIFEST, SimpleFileOptions::default())
                .unwrap();
            zip.write_all(
                br#"{"creationDate":"x","version":1,"name":"x","type":0,"userDataBackup":
                    {"lastModifiedDate":"x","hash":"x","databaseName":"userData.db",
                     "deviceName":"x","schemaVersion":16}}"#,
            )
            .unwrap();
            zip.start_file(DB_NAME, SimpleFileOptions::default())
                .unwrap();
            zip.write_all(&fs::read(&db).unwrap()).unwrap();
            zip.finish().unwrap();
        }
        assert!(local.merge_backup(&nonote).is_err());
        // Both were refused before anything was touched.
        assert!(!safety.exists());

        // A backup that fails halfway: the locations are merged before the
        // marks, whose table lacks a column.
        let mut remote = UserData::open(tmp.path().join("r").join(DB_NAME)).unwrap();
        remote
            .add_mark(&bible(2), None, 2, &[verse(1, 0, 3)])
            .unwrap();
        remote
            .conn
            .execute_batch("ALTER TABLE UserMark DROP COLUMN Version")
            .unwrap();
        let broken = backup_of(&remote, tmp.path(), "broken.jwlibrary");
        let err = local.merge_backup(&broken).unwrap_err();
        assert!(err.to_string().contains("Version"), "{err}");
        // It got past the checks, so the new Location was rolled back.
        assert!(safety.exists());

        assert_eq!(counts(&local), before);
        assert_eq!(local.marks(&bible(1)).unwrap()[0].color, 4);
        let mut fk = local.conn.prepare("PRAGMA foreign_key_check").unwrap();
        assert!(fk.query([]).unwrap().next().unwrap().is_none());
        drop(fk);
        assert_eq!(attached(&local), ["main"]);
        let leftovers = fs::read_dir(tmp.path().join("l"))
            .unwrap()
            .filter(|e| {
                e.as_ref()
                    .unwrap()
                    .file_name()
                    .to_string_lossy()
                    .starts_with(".merge-")
            })
            .count();
        assert_eq!(leftovers, 0);
        // The data still works after the failed merges.
        local
            .add_mark(&bible(3), None, 1, &[verse(1, 0, 3)])
            .unwrap();
    }

    #[test]
    fn merge_location_with_the_same_unique_key_is_reused() {
        // A Bible chapter that differs only outside the UNIQUE key is the
        // same row: inserting it would break the constraint.
        let variants = [
            Loc {
                issue_tag: 5,
                ..bible(1)
            },
            Loc {
                document_id: Some(99),
                ..bible(1)
            },
        ];
        for loc in variants {
            let tmp = tempfile::tempdir().unwrap();
            let mut local = UserData::open(tmp.path().join("l").join(DB_NAME)).unwrap();
            let mut remote = UserData::open(tmp.path().join("r").join(DB_NAME)).unwrap();
            local
                .add_mark(&bible(1), None, 1, &[verse(1, 0, 3)])
                .unwrap();
            let guid = remote.add_mark(&loc, None, 2, &[verse(5, 0, 3)]).unwrap();
            let r = local
                .merge_backup(&backup_of(&remote, tmp.path(), "r.jwlibrary"))
                .unwrap();
            assert_eq!(r.added, 1);
            assert_eq!(count(&local, "Location"), 1);
            let marks = local.marks(&bible(1)).unwrap();
            assert_eq!(marks.len(), 2);
            assert_eq!(marks[1].guid, guid);
        }
    }

    #[test]
    fn merge_location_with_the_second_unique_key_is_reused() {
        // JW Library also keeps (KeySymbol, IssueTagNumber, MepsLanguage,
        // DocumentId, Track, Type) unique.
        let tmp = tempfile::tempdir().unwrap();
        let mut local = UserData::open(tmp.path().join("l").join(DB_NAME)).unwrap();
        let remote = UserData::open(tmp.path().join("r").join(DB_NAME)).unwrap();
        for (ud, book) in [(&local, None), (&remote, Some(3))] {
            ud.conn
                .execute(
                    "INSERT INTO Location (BookNumber, DocumentId, Track, IssueTagNumber,
                                           KeySymbol, MepsLanguage, Type)
                     VALUES (?1, 5, 1, 0, 'sjjm', 2, 2)",
                    [book],
                )
                .unwrap();
        }
        local
            .merge_backup(&backup_of(&remote, tmp.path(), "r.jwlibrary"))
            .unwrap();
        assert_eq!(count(&local, "Location"), 1);
    }

    #[test]
    fn merge_matches_locations_with_zero_or_null_like_the_editor() {
        // JW Library may store 0 where pergament stores NULL: the same page.
        for zeros_locally in [true, false] {
            let tmp = tempfile::tempdir().unwrap();
            let mut local = UserData::open(tmp.path().join("l").join(DB_NAME)).unwrap();
            let mut remote = UserData::open(tmp.path().join("r").join(DB_NAME)).unwrap();
            let (zeros, nulls) = if zeros_locally {
                (&mut local, &mut remote)
            } else {
                (&mut remote, &mut local)
            };
            zeros
                .conn
                .execute(
                    "INSERT INTO Location (BookNumber, ChapterNumber, DocumentId, IssueTagNumber,
                                           KeySymbol, MepsLanguage, Type)
                     VALUES (0, 0, 2026520, 20260800, 'w', 2, 0)",
                    [],
                )
                .unwrap();
            zeros
                .save_answer(&watchtower(), None, "tt1", "zeros")
                .unwrap();
            nulls
                .save_answer(&watchtower(), None, "tt1", "nulls")
                .unwrap();
            nulls
                .save_answer(&watchtower(), None, "tt2", "only nulls")
                .unwrap();
            assert_eq!(count(zeros, "Location"), 1);
            set_db_time(&local, "2020-01-01T00:00:00Z");
            set_db_time(&remote, "2030-01-01T00:00:00Z");
            let r = local
                .merge_backup(&backup_of(&remote, tmp.path(), "r.jwlibrary"))
                .unwrap();
            assert_eq!(
                count(&local, "Location"),
                1,
                "zeros locally: {zeros_locally}"
            );
            assert_eq!(r.updated, 1);
            assert!(r.conflicts.is_empty());
            let answers = local.answers(&watchtower()).unwrap();
            let remote_value = if zeros_locally { "nulls" } else { "zeros" };
            assert_eq!(answers["tt1"], remote_value);
            if zeros_locally {
                assert_eq!(r.added, 1);
                assert_eq!(answers["tt2"], "only nulls");
            }
            assert_foreign_keys_hold(&local);
        }
    }

    #[test]
    fn merge_answer_on_a_location_without_language_meets_the_shown_one() {
        // The editor shows the answers of a page from its own location and
        // from one without a language; a merged answer must not hide behind
        // the local one of the same field.
        for remote_newer in [true, false] {
            let tmp = tempfile::tempdir().unwrap();
            let mut local = UserData::open(tmp.path().join("l").join(DB_NAME)).unwrap();
            let remote = UserData::open(tmp.path().join("r").join(DB_NAME)).unwrap();
            local
                .save_answer(&watchtower(), None, "tt1", "local")
                .unwrap();
            remote
                .conn
                .execute_batch(
                    "INSERT INTO Location (LocationId, DocumentId, IssueTagNumber, KeySymbol, Type)
                     VALUES (7, 2026520, 20260800, 'w', 0);
                     INSERT INTO InputField VALUES (7, 'tt1', 'remote');",
                )
                .unwrap();
            let (old, new) = ("2020-01-01T00:00:00Z", "2030-01-01T00:00:00Z");
            set_db_time(&local, if remote_newer { old } else { new });
            set_db_time(&remote, if remote_newer { new } else { old });
            let b = backup_of(&remote, tmp.path(), "r.jwlibrary");
            let r = local.merge_backup(&b).unwrap();
            assert_eq!(r.added, 0);
            assert_eq!(r.updated, i64::from(remote_newer));
            assert_eq!(r.conflicts.len(), usize::from(!remote_newer));
            let expected = if remote_newer { "remote" } else { "local" };
            assert_eq!(local.answers(&watchtower()).unwrap()["tt1"], expected);
            assert_eq!(count(&local, "InputField"), 1);
            if remote_newer {
                let again = local.merge_backup(&b).unwrap();
                assert_eq!((again.added, again.updated, again.unchanged), (0, 0, 1));
            }
        }
    }

    #[test]
    fn merge_answer_hidden_in_the_backup_does_not_replace_the_shown_one() {
        let tmp = tempfile::tempdir().unwrap();
        let mut local = UserData::open(tmp.path().join("l").join(DB_NAME)).unwrap();
        let mut remote = UserData::open(tmp.path().join("r").join(DB_NAME)).unwrap();
        local
            .save_answer(&watchtower(), None, "tt1", "old")
            .unwrap();
        remote
            .save_answer(&watchtower(), None, "tt1", "new")
            .unwrap();
        // A second, older text of the field that the remote editor does not show.
        remote
            .conn
            .execute_batch(
                "INSERT INTO Location (LocationId, DocumentId, IssueTagNumber, KeySymbol, Type)
                 VALUES (7, 2026520, 20260800, 'w', 0);
                 INSERT INTO InputField VALUES (7, 'tt1', 'stale');",
            )
            .unwrap();
        assert_eq!(remote.answers(&watchtower()).unwrap()["tt1"], "new");
        set_db_time(&local, "2020-01-01T00:00:00Z");
        set_db_time(&remote, "2030-01-01T00:00:00Z");
        let r = local
            .merge_backup(&backup_of(&remote, tmp.path(), "r.jwlibrary"))
            .unwrap();
        assert_eq!((r.added, r.updated), (0, 1));
        assert!(r.conflicts.is_empty());
        assert_eq!(local.answers(&watchtower()).unwrap()["tt1"], "new");
    }

    #[test]
    fn merge_clock_becomes_the_later_of_both() {
        let tmp = tempfile::tempdir().unwrap();
        let mut local = UserData::open(tmp.path().join("l").join(DB_NAME)).unwrap();
        let mut remote = UserData::open(tmp.path().join("r").join(DB_NAME)).unwrap();
        local
            .add_mark(&bible(1), None, 1, &[verse(1, 0, 3)])
            .unwrap();
        remote
            .add_mark(&bible(2), None, 2, &[verse(1, 0, 3)])
            .unwrap();
        set_db_time(&local, "2020-01-01T00:00:00Z");
        set_db_time(&remote, "2030-01-01T00:00:00Z");
        let r = local
            .merge_backup(&backup_of(&remote, tmp.path(), "r.jwlibrary"))
            .unwrap();
        assert_eq!(r.added, 1);
        assert_eq!(db_time(&local), "2030-01-01T00:00:00Z");
        // An older backup with something new does not move the clock.
        remote
            .add_mark(&bible(3), None, 2, &[verse(1, 0, 3)])
            .unwrap();
        set_db_time(&local, "2040-01-01T00:00:00Z");
        set_db_time(&remote, "2035-01-01T00:00:00Z");
        let r = local
            .merge_backup(&backup_of(&remote, tmp.path(), "r2.jwlibrary"))
            .unwrap();
        assert_eq!(r.added, 1);
        assert_eq!(db_time(&local), "2040-01-01T00:00:00Z");
    }

    #[test]
    fn merge_back_and_forth_loses_no_answer_silently() {
        let tmp = tempfile::tempdir().unwrap();
        let mut a = UserData::open(tmp.path().join("a").join(DB_NAME)).unwrap();
        a.save_answer(&watchtower(), None, "tt1", "seed").unwrap();
        let mut b = UserData::open(tmp.path().join("b").join(DB_NAME)).unwrap();
        b.restore_backup(&backup_of(&a, tmp.path(), "seed.jwlibrary"))
            .unwrap();
        a.save_answer(&watchtower(), None, "tt1", "A").unwrap();
        set_db_time(&a, "2025-01-01T00:00:00Z");
        b.save_answer(&watchtower(), None, "tt1", "B").unwrap();
        set_db_time(&b, "2026-01-01T00:00:00Z");

        // A merges an old backup of a third device that has something new:
        // that must not make A look newer than B.
        let mut c = UserData::open(tmp.path().join("c").join(DB_NAME)).unwrap();
        c.add_mark(&bible(1), None, 1, &[verse(1, 0, 3)]).unwrap();
        set_db_time(&c, "2010-01-01T00:00:00Z");
        let r = a
            .merge_backup(&backup_of(&c, tmp.path(), "c.jwlibrary"))
            .unwrap();
        assert_eq!(r.added, 1);
        assert_eq!(db_time(&a), "2025-01-01T00:00:00Z");

        // A merges B: B typed later and wins.
        let r = a
            .merge_backup(&backup_of(&b, tmp.path(), "b1.jwlibrary"))
            .unwrap();
        assert_eq!((r.added, r.updated), (0, 1));
        assert!(r.conflicts.is_empty());
        assert_eq!(a.answers(&watchtower()).unwrap()["tt1"], "B");
        assert_eq!(db_time(&a), "2026-01-01T00:00:00Z");

        // B merges A: the same answer, and the mark from C.
        let r = b
            .merge_backup(&backup_of(&a, tmp.path(), "a1.jwlibrary"))
            .unwrap();
        assert_eq!((r.added, r.updated, r.unchanged), (1, 0, 1));
        assert!(r.conflicts.is_empty());
        assert_eq!(b.answers(&watchtower()).unwrap()["tt1"], "B");

        // Both type again, A later. A keeps its answer and reports B's.
        b.save_answer(&watchtower(), None, "tt1", "B2").unwrap();
        set_db_time(&b, "2027-01-01T00:00:00Z");
        a.save_answer(&watchtower(), None, "tt1", "A2").unwrap();
        set_db_time(&a, "2028-01-01T00:00:00Z");
        let from_b = backup_of(&b, tmp.path(), "b2.jwlibrary");
        let r = a.merge_backup(&from_b).unwrap();
        assert_eq!((r.added, r.updated), (0, 0));
        assert_eq!(r.conflicts.len(), 1);
        assert_eq!(r.conflicts[0].table, "InputField");
        assert_eq!(a.answers(&watchtower()).unwrap()["tt1"], "A2");
        assert_eq!(db_time(&a), "2028-01-01T00:00:00Z");
        // Merging the same backup again reports it again: the rows still differ.
        let again = a.merge_backup(&from_b).unwrap();
        assert_eq!((again.added, again.updated), (0, 0));
        assert_eq!(again.conflicts, r.conflicts);

        // B merges A: A is newer, so B takes A's answer.
        let r = b
            .merge_backup(&backup_of(&a, tmp.path(), "a2.jwlibrary"))
            .unwrap();
        assert_eq!((r.added, r.updated), (0, 1));
        assert!(r.conflicts.is_empty());
        assert_eq!(b.answers(&watchtower()).unwrap()["tt1"], "A2");
        assert_eq!(db_time(&b), "2028-01-01T00:00:00Z");

        // Now both are the same: nothing more to merge either way.
        let r = a
            .merge_backup(&backup_of(&b, tmp.path(), "b3.jwlibrary"))
            .unwrap();
        assert_eq!((r.added, r.updated), (0, 0));
        assert!(r.conflicts.is_empty());
    }

    #[test]
    fn merge_identical_highlights_with_different_guids_are_unchanged() {
        for remote_newer in [true, false] {
            let tmp = tempfile::tempdir().unwrap();
            let mut local = UserData::open(tmp.path().join("l").join(DB_NAME)).unwrap();
            let mut remote = UserData::open(tmp.path().join("r").join(DB_NAME)).unwrap();
            let mine = local
                .add_mark(&bible(1), None, 3, &[verse(1, 0, 3), verse(2, 1, 4)])
                .unwrap();
            location(&remote, Some(50), 0);
            let theirs = remote
                .add_mark(&bible(1), None, 3, &[verse(1, 0, 3), verse(2, 1, 4)])
                .unwrap();
            let linked = remote
                .save_note(
                    Some(&bible(1)),
                    None,
                    &NoteInput {
                        mark_guid: Some(theirs),
                        ..note_input("N", "n", &[])
                    },
                )
                .unwrap();
            let (old, new) = ("2020-01-01T00:00:00Z", "2030-01-01T00:00:00Z");
            set_db_time(&local, if remote_newer { old } else { new });
            set_db_time(&remote, if remote_newer { new } else { old });
            let b = backup_of(&remote, tmp.path(), "r.jwlibrary");
            for _ in 0..2 {
                let r = local.merge_backup(&b).unwrap();
                assert!(r.conflicts.is_empty(), "remote newer: {remote_newer}");
                assert_eq!(count(&local, "UserMark"), 1);
                assert_eq!(count(&local, "BlockRange"), 2);
            }
            let marks = local.marks(&bible(1)).unwrap();
            assert_eq!(marks[0].guid, mine);
            // The note on the remote highlight sits on the local twin.
            assert_eq!(note(&local, &linked).mark_guid, Some(mine));
        }
    }

    #[test]
    fn merge_winning_note_keeps_its_highlight_when_the_backup_one_is_skipped() {
        let tmp = tempfile::tempdir().unwrap();
        let mut local = UserData::open(tmp.path().join("l").join(DB_NAME)).unwrap();
        let mark = local
            .add_mark(&bible(1), None, 1, &[verse(1, 0, 9)])
            .unwrap();
        let guid = local
            .save_note(
                Some(&bible(1)),
                None,
                &NoteInput {
                    mark_guid: Some(mark.clone()),
                    ..note_input("T", "old", &[])
                },
            )
            .unwrap();
        let mut remote = UserData::open(tmp.path().join("r").join(DB_NAME)).unwrap();
        remote
            .restore_backup(&backup_of(&local, tmp.path(), "seed.jwlibrary"))
            .unwrap();
        // The remote moves the note onto a new, overlapping highlight.
        let other = remote
            .add_mark(&bible(1), None, 2, &[verse(1, 5, 12)])
            .unwrap();
        remote
            .conn
            .execute(
                "UPDATE Note SET Content = 'new',
                     UserMarkId = (SELECT UserMarkId FROM UserMark WHERE UserMarkGuid = ?1)",
                [&other],
            )
            .unwrap();
        set_note_time(&remote, &guid, "2030-01-01T00:00:00Z");
        set_note_time(&local, &guid, "2020-01-01T00:00:00Z");
        // The local database is newer, so the overlapping highlight is skipped.
        set_db_time(&remote, "2020-01-01T00:00:00Z");
        set_db_time(&local, "2030-01-01T00:00:00Z");
        let r = local
            .merge_backup(&backup_of(&remote, tmp.path(), "r.jwlibrary"))
            .unwrap();
        assert!(r.conflicts.iter().any(|c| c.guid == other));
        let merged = note(&local, &guid);
        assert_eq!(merged.content, "new");
        assert_eq!(merged.mark_guid, Some(mark));
    }

    #[test]
    fn merge_winning_note_keeps_its_place_when_the_backup_location_is_missing() {
        let tmp = tempfile::tempdir().unwrap();
        let mut local = UserData::open(tmp.path().join("l").join(DB_NAME)).unwrap();
        let guid = local
            .save_note(Some(&bible(1)), None, &note_input("T", "old", &[]))
            .unwrap();
        local
            .conn
            .execute("UPDATE Note SET BlockType = 2, BlockIdentifier = 3", [])
            .unwrap();
        let before = note(&local, &guid).location;
        assert!(before.is_some());
        let mut remote = UserData::open(tmp.path().join("r").join(DB_NAME)).unwrap();
        remote
            .restore_backup(&backup_of(&local, tmp.path(), "seed.jwlibrary"))
            .unwrap();
        // The backup note points at a location its database does not have.
        remote
            .conn
            .execute_batch(
                "PRAGMA foreign_keys = OFF;
                 UPDATE Note SET Content = 'new', LocationId = 9999, BlockType = 2, BlockIdentifier = 7;",
            )
            .unwrap();
        set_note_time(&remote, &guid, "2030-01-01T00:00:00Z");
        set_note_time(&local, &guid, "2020-01-01T00:00:00Z");
        local
            .merge_backup(&backup_of(&remote, tmp.path(), "r.jwlibrary"))
            .unwrap();
        let merged = note(&local, &guid);
        assert_eq!(merged.content, "new");
        assert_eq!(merged.location, before);
        assert_eq!(merged.block_identifier, Some(3));
    }

    #[test]
    fn merge_tolerates_a_stale_attached_database() {
        let tmp = tempfile::tempdir().unwrap();
        let mut local = UserData::open(tmp.path().join("l").join(DB_NAME)).unwrap();
        let remote = UserData::open(tmp.path().join("r").join(DB_NAME)).unwrap();
        local
            .conn
            .execute("ATTACH DATABASE ':memory:' AS inc", [])
            .unwrap();
        local
            .merge_backup(&backup_of(&remote, tmp.path(), "r.jwlibrary"))
            .unwrap();
        let attached: i64 = local
            .conn
            .query_row(
                "SELECT count(*) FROM pragma_database_list WHERE name = 'inc'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(attached, 0);
    }

    fn set_note_time(ud: &UserData, guid: &str, ts: &str) {
        ud.conn
            .execute(
                "UPDATE Note SET LastModified = ?2 WHERE Guid = ?1",
                [guid, ts],
            )
            .unwrap();
    }

    fn note_input(title: &str, content: &str, tags: &[&str]) -> NoteInput {
        NoteInput {
            title: title.into(),
            content: content.into(),
            tags: tags.iter().map(|t| t.to_string()).collect(),
            ..Default::default()
        }
    }

    fn edit_note(ud: &mut UserData, guid: &str, title: &str, content: &str, tags: &[&str]) {
        let note = NoteInput {
            guid: Some(guid.into()),
            ..note_input(title, content, tags)
        };
        ud.save_note(None, None, &note).unwrap();
    }

    fn note(ud: &UserData, guid: &str) -> Note {
        ud.all_notes(None)
            .unwrap()
            .into_iter()
            .find(|n| n.guid == guid)
            .unwrap()
    }

    fn assert_foreign_keys_hold(ud: &UserData) {
        let mut fk = ud.conn.prepare("PRAGMA foreign_key_check").unwrap();
        assert!(fk.query([]).unwrap().next().unwrap().is_none());
    }

    /// The `LocationId` of a `nwtsty` row (book 19), adding it when missing.
    fn location(ud: &UserData, chapter: Option<i64>, kind: i64) -> i64 {
        let book = chapter.map(|_| 19);
        let found: Option<i64> = ud
            .conn
            .query_row(
                "SELECT LocationId FROM Location WHERE BookNumber IS ?1 AND ChapterNumber IS ?2
                   AND KeySymbol = 'nwtsty' AND MepsLanguage = 2 AND Type = ?3",
                params![book, chapter, kind],
                |r| r.get(0),
            )
            .optional()
            .unwrap();
        found.unwrap_or_else(|| {
            ud.conn
                .execute(
                    "INSERT INTO Location (BookNumber, ChapterNumber, KeySymbol, MepsLanguage, Type)
                     VALUES (?1, ?2, 'nwtsty', 2, ?3)",
                    params![book, chapter, kind],
                )
                .unwrap();
            ud.conn.last_insert_rowid()
        })
    }

    /// Point bookmark `slot` of the Bible at `chapter`.
    fn put_bookmark(ud: &UserData, slot: i64, chapter: i64, title: &str) {
        let publication = location(ud, None, 1);
        let page = location(ud, Some(chapter), 0);
        ud.conn
            .execute(
                "INSERT INTO Bookmark (LocationId, PublicationLocationId, Slot, Title, BlockType)
                 VALUES (?1, ?2, ?3, ?4, 0)
                 ON CONFLICT (PublicationLocationId, Slot)
                 DO UPDATE SET LocationId = excluded.LocationId, Title = excluded.Title",
                params![page, publication, slot, title],
            )
            .unwrap();
    }

    /// Bookmarks as (slot, chapter, title).
    fn bookmark_slots(ud: &UserData) -> Vec<(i64, i64, String)> {
        ud.bookmarks()
            .unwrap()
            .into_iter()
            .map(|b| (b.slot, b.location.loc.chapter.unwrap(), b.title))
            .collect()
    }

    #[test]
    fn merge_adds_notes_with_tags_and_links_to_marks() {
        let tmp = tempfile::tempdir().unwrap();
        let mut local = UserData::open(tmp.path().join("l").join(DB_NAME)).unwrap();
        let mut remote = UserData::open(tmp.path().join("r").join(DB_NAME)).unwrap();
        local
            .save_note(
                Some(&bible(1)),
                None,
                &note_input("Local", "L", &["Prayer"]),
            )
            .unwrap();
        // Shift the remote ids so nothing lines up by accident.
        location(&remote, Some(50), 0);
        let mark = remote
            .add_mark(&bible(3), Some("Psalm 3"), 4, &[verse(2, 0, 5)])
            .unwrap();
        let guid = remote
            .save_note(
                Some(&bible(3)),
                None,
                &NoteInput {
                    block_type: BLOCK_VERSE,
                    block_identifier: Some(2),
                    mark_guid: Some(mark.clone()),
                    ..note_input("Shield", "Text", &["Study", "Prayer"])
                },
            )
            .unwrap();
        let r = local
            .merge_backup(&backup_of(&remote, tmp.path(), "r.jwlibrary"))
            .unwrap();
        // The mark and the note.
        assert_eq!((r.added, r.updated, r.unchanged), (2, 0, 0));
        assert!(r.conflicts.is_empty());
        let notes = local.notes(&bible(3)).unwrap();
        assert_eq!(notes.len(), 1);
        let merged = &notes[0];
        assert_eq!(merged.guid, guid);
        assert_eq!(
            (merged.title.as_str(), merged.content.as_str()),
            ("Shield", "Text")
        );
        assert_eq!(merged.tags, ["Prayer", "Study"]);
        assert_eq!(merged.mark_guid.as_deref(), Some(mark.as_str()));
        assert_eq!(merged.color, Some(4));
        assert_eq!(
            (merged.block_type, merged.block_identifier),
            (BLOCK_VERSE, Some(2))
        );
        assert_eq!(
            merged.location.as_ref().unwrap().title.as_deref(),
            Some("Psalm 3")
        );
        assert_eq!(merged.last_modified, note(&remote, &guid).last_modified);
        assert_eq!(local.notes(&bible(1)).unwrap()[0].tags, ["Prayer"]);
        assert_eq!(count(&local, "Tag"), 2);
        assert_foreign_keys_hold(&local);
    }

    #[test]
    fn merge_newer_note_wins_by_its_own_timestamp() {
        let tmp = tempfile::tempdir().unwrap();
        let mut local = UserData::open(tmp.path().join("l").join(DB_NAME)).unwrap();
        let guid = local
            .save_note(Some(&bible(1)), None, &note_input("Old", "old", &["Local"]))
            .unwrap();
        let mut remote = UserData::open(tmp.path().join("r").join(DB_NAME)).unwrap();
        remote
            .restore_backup(&backup_of(&local, tmp.path(), "seed.jwlibrary"))
            .unwrap();
        edit_note(&mut remote, &guid, "New", "new", &["Remote"]);
        set_note_time(&remote, &guid, "2030-01-01T00:00:00Z");
        set_note_time(&local, &guid, "2020-01-01T00:00:00Z");
        // The database clocks say the opposite: the note's own time decides.
        set_db_time(&remote, "2000-01-01T00:00:00Z");
        set_db_time(&local, "2040-01-01T00:00:00Z");
        let r = local
            .merge_backup(&backup_of(&remote, tmp.path(), "r.jwlibrary"))
            .unwrap();
        assert_eq!((r.added, r.updated, r.unchanged), (0, 1, 0));
        let merged = note(&local, &guid);
        assert_eq!(
            (merged.title.as_str(), merged.content.as_str()),
            ("New", "new")
        );
        // The winning note brings its tags along.
        assert_eq!(merged.tags, ["Remote"]);
        assert_eq!(merged.last_modified, "2030-01-01T00:00:00Z");

        // Now the local note is newer, though the remote database is.
        edit_note(&mut local, &guid, "Mine", "mine", &["Local"]);
        set_note_time(&local, &guid, "2040-01-01T00:00:00Z");
        set_db_time(&local, "2000-01-01T00:00:00Z");
        set_db_time(&remote, "2050-01-01T00:00:00Z");
        let r = local
            .merge_backup(&backup_of(&remote, tmp.path(), "r2.jwlibrary"))
            .unwrap();
        assert_eq!((r.added, r.updated, r.unchanged), (0, 0, 1));
        assert!(r.conflicts.is_empty());
        let kept = note(&local, &guid);
        assert_eq!(
            (kept.title.as_str(), kept.content.as_str()),
            ("Mine", "mine")
        );
        assert_eq!(kept.tags, ["Local"]);
        assert_eq!(count(&local, "Note"), 1);
    }

    #[test]
    fn merge_note_equal_timestamp_different_content_is_a_conflict() {
        let tmp = tempfile::tempdir().unwrap();
        let mut local = UserData::open(tmp.path().join("l").join(DB_NAME)).unwrap();
        let guid = local
            .save_note(Some(&bible(1)), None, &note_input("Psalm", "a", &["One"]))
            .unwrap();
        let mut remote = UserData::open(tmp.path().join("r").join(DB_NAME)).unwrap();
        remote
            .restore_backup(&backup_of(&local, tmp.path(), "seed.jwlibrary"))
            .unwrap();
        edit_note(&mut remote, &guid, "Psalm", "b", &["Two"]);
        // The same instant, written two ways.
        set_note_time(&remote, &guid, "2030-01-01T00:00:00+00:00");
        set_note_time(&local, &guid, "2030-01-01T00:00:00Z");
        let r = local
            .merge_backup(&backup_of(&remote, tmp.path(), "r.jwlibrary"))
            .unwrap();
        assert_eq!((r.added, r.updated, r.unchanged), (0, 0, 0));
        assert_eq!(r.conflicts.len(), 1);
        assert_eq!(r.conflicts[0].table, "Note");
        assert_eq!(r.conflicts[0].guid, guid);
        assert_eq!(r.conflicts[0].summary, "Psalm");
        let kept = note(&local, &guid);
        assert_eq!(kept.content, "a");
        assert_eq!(kept.tags, ["One"]);
    }

    #[test]
    fn merge_untitled_note_conflict_shows_the_start_of_the_text() {
        for (content, summary) in [
            ("  Short\n text ", "Short text"),
            (
                "A rather long note that goes on and on about many things",
                "A rather long note that goes on and on a…",
            ),
        ] {
            let tmp = tempfile::tempdir().unwrap();
            let mut local = UserData::open(tmp.path().join("l").join(DB_NAME)).unwrap();
            let guid = local
                .save_note(Some(&bible(1)), None, &note_input("", content, &[]))
                .unwrap();
            let mut remote = UserData::open(tmp.path().join("r").join(DB_NAME)).unwrap();
            remote
                .restore_backup(&backup_of(&local, tmp.path(), "seed.jwlibrary"))
                .unwrap();
            edit_note(&mut remote, &guid, "", "other", &[]);
            set_note_time(&remote, &guid, "2030-01-01T00:00:00Z");
            set_note_time(&local, &guid, "2030-01-01T00:00:00Z");
            let r = local
                .merge_backup(&backup_of(&remote, tmp.path(), "r.jwlibrary"))
                .unwrap();
            assert_eq!(r.conflicts.len(), 1);
            assert_eq!(r.conflicts[0].summary, summary);
        }
    }

    #[test]
    fn merge_note_timestamps_with_offset_compare_correctly() {
        // (remote, local, remote wins): the second pair sorts the wrong way
        // as text, since 01:00+02:00 is 23:00 the day before.
        for (remote_time, local_time, remote_wins) in [
            ("2030-01-01T00:00:00+00:00", "2029-12-31T23:59:59Z", true),
            ("2030-01-01T01:00:00+02:00", "2029-12-31T23:30:00Z", false),
        ] {
            let tmp = tempfile::tempdir().unwrap();
            let mut local = UserData::open(tmp.path().join("l").join(DB_NAME)).unwrap();
            let guid = local
                .save_note(Some(&bible(1)), None, &note_input("T", "local", &[]))
                .unwrap();
            let mut remote = UserData::open(tmp.path().join("r").join(DB_NAME)).unwrap();
            remote
                .restore_backup(&backup_of(&local, tmp.path(), "seed.jwlibrary"))
                .unwrap();
            edit_note(&mut remote, &guid, "T", "remote", &[]);
            set_note_time(&remote, &guid, remote_time);
            set_note_time(&local, &guid, local_time);
            let r = local
                .merge_backup(&backup_of(&remote, tmp.path(), "r.jwlibrary"))
                .unwrap();
            assert_eq!(r.updated, i64::from(remote_wins), "{remote_time}");
            assert!(r.conflicts.is_empty());
            let expected = if remote_wins { "remote" } else { "local" };
            assert_eq!(note(&local, &guid).content, expected);
        }
    }

    #[test]
    fn merge_tag_with_different_ids_maps_by_name() {
        let tmp = tempfile::tempdir().unwrap();
        let mut local = UserData::open(tmp.path().join("l").join(DB_NAME)).unwrap();
        let mut remote = UserData::open(tmp.path().join("r").join(DB_NAME)).unwrap();
        let mine = local
            .save_note(Some(&bible(1)), None, &note_input("Mine", "m", &["Prayer"]))
            .unwrap();
        // Remote: "Study" is TagId 1 and "Prayer" TagId 2.
        let study = remote
            .save_note(Some(&bible(2)), None, &note_input("S", "s", &["Study"]))
            .unwrap();
        let both = remote
            .save_note(
                Some(&bible(3)),
                None,
                &note_input("B", "b", &["Prayer", "Study"]),
            )
            .unwrap();
        let tag_id = |ud: &UserData, name: &str| -> i64 {
            ud.conn
                .query_row("SELECT TagId FROM Tag WHERE Name = ?1", [name], |r| {
                    r.get(0)
                })
                .unwrap()
        };
        assert_eq!((tag_id(&local, "Prayer"), tag_id(&remote, "Study")), (1, 1));
        assert_eq!(tag_id(&remote, "Prayer"), 2);
        let r = local
            .merge_backup(&backup_of(&remote, tmp.path(), "r.jwlibrary"))
            .unwrap();
        assert_eq!(r.added, 2);
        let names: Vec<String> = local.tags().unwrap().into_iter().map(|t| t.name).collect();
        assert_eq!(names, ["Prayer", "Study"]);
        assert_eq!(count(&local, "Tag"), 2);
        assert_eq!(note(&local, &mine).tags, ["Prayer"]);
        assert_eq!(note(&local, &study).tags, ["Study"]);
        assert_eq!(note(&local, &both).tags, ["Prayer", "Study"]);
        let prayer = tag_id(&local, "Prayer");
        assert_eq!(local.all_notes(Some(prayer)).unwrap().len(), 2);
        assert_foreign_keys_hold(&local);
    }

    #[test]
    fn merge_tagmap_positions_stay_unique() {
        let tmp = tempfile::tempdir().unwrap();
        let mut local = UserData::open(tmp.path().join("l").join(DB_NAME)).unwrap();
        let mut remote = UserData::open(tmp.path().join("r").join(DB_NAME)).unwrap();
        for (ud, prefix) in [(&mut local, "L"), (&mut remote, "R")] {
            for chapter in 1..=3 {
                ud.save_note(
                    Some(&bible(chapter)),
                    None,
                    &note_input(&format!("{prefix}{chapter}"), "x", &["Prayer", "Study"]),
                )
                .unwrap();
            }
        }
        // Favorites (tag type 0) on locations, partly the same on both sides.
        for (ud, chapters) in [(&local, [1, 2]), (&remote, [2, 4])] {
            ud.conn
                .execute("INSERT INTO Tag (Type, Name) VALUES (0, 'Favorite')", [])
                .unwrap();
            let favorite = ud.conn.last_insert_rowid();
            for (position, chapter) in chapters.into_iter().enumerate() {
                let page = location(ud, Some(chapter), 0);
                ud.conn
                    .execute(
                        "INSERT INTO TagMap (LocationId, TagId, Position) VALUES (?1, ?2, ?3)",
                        params![page, favorite, position as i64],
                    )
                    .unwrap();
            }
        }
        let r = local
            .merge_backup(&backup_of(&remote, tmp.path(), "r.jwlibrary"))
            .unwrap();
        // Three notes and the favorite of chapter 4; chapter 2 was there.
        assert_eq!((r.added, r.unchanged), (4, 1));
        let (rows, distinct): (i64, i64) = local
            .conn
            .query_row(
                "SELECT count(*), count(DISTINCT TagId || '-' || Position) FROM TagMap",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        // 6 notes x 2 tags + 3 favorites.
        assert_eq!((rows, distinct), (15, 15));
        let favorites: i64 = local
            .conn
            .query_row(
                "SELECT count(*) FROM TagMap m JOIN Tag t ON t.TagId = m.TagId
                 WHERE t.Type = 0 AND m.LocationId IS NOT NULL",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(favorites, 3);
        assert_eq!(count(&local, "Tag"), 3);
        assert_foreign_keys_hold(&local);
    }

    #[test]
    fn merge_bookmark_slots() {
        let (old, new) = ("2020-01-01T00:00:00Z", "2030-01-01T00:00:00Z");
        for (local_time, remote_time, slot0) in [
            (old, new, (0, 2, "Remote")),
            (new, old, (0, 1, "Local")),
            (new, new, (0, 1, "Local")),
        ] {
            let tmp = tempfile::tempdir().unwrap();
            let local = UserData::open(tmp.path().join("l").join(DB_NAME)).unwrap();
            let remote = UserData::open(tmp.path().join("r").join(DB_NAME)).unwrap();
            put_bookmark(&local, 0, 1, "Local");
            // Different LocationIds on the remote side.
            location(&remote, Some(50), 0);
            put_bookmark(&remote, 0, 2, "Remote");
            put_bookmark(&remote, 1, 3, "Free slot");
            set_db_time(&local, local_time);
            set_db_time(&remote, remote_time);
            let mut local = local;
            let r = local
                .merge_backup(&backup_of(&remote, tmp.path(), "r.jwlibrary"))
                .unwrap();
            let (slot, chapter, title) = slot0;
            assert_eq!(
                bookmark_slots(&local),
                [
                    (slot, chapter, title.to_owned()),
                    (1, 3, "Free slot".to_owned())
                ]
            );
            assert_eq!(r.added, 1);
            assert_eq!(r.updated, i64::from(remote_time > local_time));
            // Kept, whether the backup is older or as old: reported.
            if remote_time <= local_time {
                assert_eq!(r.conflicts.len(), 1);
                assert_eq!(r.conflicts[0].table, "Bookmark");
                assert_eq!(r.conflicts[0].summary, "Local");
            } else {
                assert!(r.conflicts.is_empty());
            }
            // One publication location, chapters 1-3 and the unused 50.
            assert_eq!(count(&local, "Location"), 5);
            assert_foreign_keys_hold(&local);
        }
    }

    #[test]
    fn merge_twice_is_idempotent_with_notes() {
        let tmp = tempfile::tempdir().unwrap();
        let mut local = UserData::open(tmp.path().join("l").join(DB_NAME)).unwrap();
        let mut remote = UserData::open(tmp.path().join("r").join(DB_NAME)).unwrap();
        local
            .add_mark(&bible(1), None, 1, &[verse(1, 0, 3)])
            .unwrap();
        local
            .save_note(Some(&bible(1)), None, &note_input("L", "l", &["Prayer"]))
            .unwrap();
        put_bookmark(&local, 0, 1, "Local");
        location(&remote, Some(50), 0);
        let mark = remote
            .add_mark(&bible(2), None, 3, &[verse(4, 0, 3)])
            .unwrap();
        remote
            .save_note(
                Some(&bible(2)),
                None,
                &NoteInput {
                    mark_guid: Some(mark),
                    ..note_input("R", "r", &["Study", "Prayer"])
                },
            )
            .unwrap();
        remote
            .save_note(Some(&bible(3)), None, &note_input("R2", "r2", &["Study"]))
            .unwrap();
        put_bookmark(&remote, 1, 2, "Remote");
        remote
            .conn
            .execute("INSERT INTO Tag (Type, Name) VALUES (0, 'Favorite')", [])
            .unwrap();
        let page = location(&remote, Some(2), 0);
        remote
            .conn
            .execute(
                "INSERT INTO TagMap (LocationId, TagId, Position)
                 VALUES (?1, (SELECT TagId FROM Tag WHERE Type = 0), 0)",
                [page],
            )
            .unwrap();
        let b = backup_of(&remote, tmp.path(), "r.jwlibrary");
        let counts = |ud: &UserData| {
            ["Note", "Tag", "TagMap", "Bookmark", "UserMark", "Location"].map(|t| count(ud, t))
        };
        let tag_map = |ud: &UserData| -> Vec<(i64, i64, Option<i64>, Option<i64>)> {
            let mut stmt = ud
                .conn
                .prepare("SELECT TagId, Position, NoteId, LocationId FROM TagMap ORDER BY 1, 2")
                .unwrap();
            stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
                .unwrap()
                .collect::<rusqlite::Result<_>>()
                .unwrap()
        };
        let first = local.merge_backup(&b).unwrap();
        // A mark, two notes, a bookmark and a favorite.
        assert_eq!((first.added, first.updated, first.unchanged), (5, 0, 0));
        let after_first = counts(&local);
        assert_eq!(after_first, [3, 3, 5, 2, 2, 5]);
        let map_first = tag_map(&local);
        set_db_time(&local, "2020-01-01T00:00:00Z");
        let second = local.merge_backup(&b).unwrap();
        assert_eq!((second.added, second.updated, second.unchanged), (0, 0, 5));
        assert!(second.conflicts.is_empty());
        assert_eq!(counts(&local), after_first);
        assert_eq!(tag_map(&local), map_first);
        assert_eq!(db_time(&local), "2020-01-01T00:00:00Z");
        assert_foreign_keys_hold(&local);
    }

    #[test]
    fn merge_round_trip_with_export() {
        let tmp = tempfile::tempdir().unwrap();
        let mut local = UserData::open(tmp.path().join("l").join(DB_NAME)).unwrap();
        let mut remote = UserData::open(tmp.path().join("r").join(DB_NAME)).unwrap();
        local
            .add_mark(&bible(1), None, 1, &[verse(1, 0, 3)])
            .unwrap();
        local
            .save_note(Some(&bible(1)), None, &note_input("L", "l", &["Prayer"]))
            .unwrap();
        put_bookmark(&local, 0, 1, "Local");
        let mark = remote
            .add_mark(&bible(2), None, 3, &[verse(4, 0, 3)])
            .unwrap();
        remote
            .save_note(
                Some(&bible(2)),
                None,
                &NoteInput {
                    mark_guid: Some(mark),
                    ..note_input("R", "r", &["Study"])
                },
            )
            .unwrap();
        put_bookmark(&remote, 1, 2, "Remote");
        local
            .merge_backup(&backup_of(&remote, tmp.path(), "r.jwlibrary"))
            .unwrap();
        let expected = local.summary().unwrap();
        assert_eq!(
            (
                expected.marks,
                expected.notes,
                expected.tags,
                expected.bookmarks
            ),
            (2, 2, 2, 2)
        );
        let mut third = UserData::open(tmp.path().join("t").join(DB_NAME)).unwrap();
        third
            .restore_backup(&backup_of(&local, tmp.path(), "merged.jwlibrary"))
            .unwrap();
        assert_eq!(third.summary().unwrap(), expected);
        assert_eq!(
            third.all_notes(None).unwrap(),
            local.all_notes(None).unwrap()
        );
        assert_eq!(third.bookmarks().unwrap(), local.bookmarks().unwrap());
        assert_foreign_keys_hold(&third);
    }

    #[test]
    fn merge_accepts_extra_tables() {
        let tmp = tempfile::tempdir().unwrap();
        let mut local = UserData::open(tmp.path().join("l").join(DB_NAME)).unwrap();
        let mut remote = UserData::open(tmp.path().join("r").join(DB_NAME)).unwrap();
        let guid = remote
            .save_note(Some(&bible(1)), None, &note_input("R", "r", &["Study"]))
            .unwrap();
        {
            let conn = Connection::open(remote.path()).unwrap();
            conn.execute_batch("CREATE TABLE Extra(x); INSERT INTO Extra VALUES (1);")
                .unwrap();
        }
        let r = local
            .merge_backup(&backup_of(&remote, tmp.path(), "r.jwlibrary"))
            .unwrap();
        assert_eq!(r.added, 1);
        assert_eq!(note(&local, &guid).tags, ["Study"]);
        let extra: bool = local
            .conn
            .query_row(
                "SELECT count(*) > 0 FROM sqlite_master WHERE name = 'Extra'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert!(!extra);
    }

    #[test]
    fn fingerprint_stable_without_changes() {
        let tmp = tempfile::tempdir().unwrap();
        let mut ud = UserData::open(tmp.path().join(DB_NAME)).unwrap();
        ud.add_mark(&bible(23), None, 3, &[verse(1, 0, 3)]).unwrap();
        let before = ud.fingerprint().unwrap();
        assert_eq!(before.len(), 64);
        ud.summary().unwrap();
        ud.export_backup(&tmp.path().join("b.jwlibrary")).unwrap();
        assert_eq!(ud.fingerprint().unwrap(), before);
    }

    #[test]
    fn fingerprint_changes_on_edit_and_delete() {
        let tmp = tempfile::tempdir().unwrap();
        let mut ud = UserData::open(tmp.path().join(DB_NAME)).unwrap();
        let empty = ud.fingerprint().unwrap();
        let guid = ud.add_mark(&bible(23), None, 3, &[verse(1, 0, 3)]).unwrap();
        let added = ud.fingerprint().unwrap();
        assert_ne!(added, empty);
        ud.delete_mark(&guid).unwrap();
        let deleted = ud.fingerprint().unwrap();
        assert_ne!(deleted, added);
        ud.add_mark(&bible(23), None, 3, &[verse(1, 0, 3)]).unwrap();
        assert_ne!(ud.fingerprint().unwrap(), empty);
    }

    #[test]
    fn fingerprint_ignores_last_modified() {
        let tmp = tempfile::tempdir().unwrap();
        let mut ud = UserData::open(tmp.path().join(DB_NAME)).unwrap();
        ud.add_mark(&bible(23), None, 3, &[verse(1, 0, 3)]).unwrap();
        let before = ud.fingerprint().unwrap();
        set_db_time(&ud, "2001-01-01T00:00:00Z");
        assert_eq!(ud.fingerprint().unwrap(), before);
    }

    #[test]
    fn fingerprint_changes_on_merge() {
        let tmp = tempfile::tempdir().unwrap();
        let mut local = UserData::open(tmp.path().join("l").join(DB_NAME)).unwrap();
        let mut remote = UserData::open(tmp.path().join("r").join(DB_NAME)).unwrap();
        remote
            .save_note(Some(&bible(3)), None, &note_input("Old", "Text", &[]))
            .unwrap();
        // The backup is older than the local database.
        set_db_time(&remote, "2001-01-01T00:00:00Z");
        set_db_time(&local, "2030-01-01T00:00:00Z");
        let backup = backup_of(&remote, tmp.path(), "r.jwlibrary");
        let before = local.fingerprint().unwrap();
        local.merge_backup(&backup).unwrap();
        assert_eq!(db_time(&local), "2030-01-01T00:00:00Z");
        assert_ne!(local.fingerprint().unwrap(), before);
    }

    #[test]
    fn export_backup_as_sets_device_name() {
        let tmp = tempfile::tempdir().unwrap();
        let mut ud = UserData::open(tmp.path().join("a").join(DB_NAME)).unwrap();
        ud.add_mark(&bible(23), None, 6, &[verse(1, 0, 3)]).unwrap();
        let backup = tmp.path().join("b.jwlibrary");
        ud.export_backup_as(&backup, "Pergament (laptop)").unwrap();
        assert_eq!(backup_device_name(&backup).unwrap(), "Pergament (laptop)");
        let mut other = UserData::open(tmp.path().join("c").join(DB_NAME)).unwrap();
        let summary = other.restore_backup(&backup).unwrap();
        assert_eq!(summary.device.as_deref(), Some("Pergament (laptop)"));
        let stamp = ud.local_stamp().unwrap();
        assert_eq!(stamp.len(), "2026-10-20-123456".len());
    }

    /// Folder sync: two devices A and B, each merging the other's file
    /// three-way against the base it stored for that device.
    mod sync_merge {
        use super::*;
        use crate::sync_snapshot::{DeleteGuard, Snapshot, SyncMerge, SyncOutcome};

        struct Device {
            name: &'static str,
            ud: UserData,
            dir: PathBuf,
            bases: HashMap<&'static str, Snapshot>,
        }

        fn device(tmp: &Path, name: &'static str) -> Device {
            let dir = tmp.join(name);
            Device {
                name,
                ud: UserData::open(dir.join(DB_NAME)).unwrap(),
                dir,
                bases: HashMap::new(),
            }
        }

        fn sync_file(from: &Device) -> PathBuf {
            let file = from.dir.join(format!("Pergament-{}.jwlibrary", from.name));
            from.ud.export_backup(&file).unwrap();
            file
        }

        /// Merge `file` into `to` with its base for `from`; a merge stores
        /// the returned snapshot as that base.
        fn merge_file(file: &Path, from: &str, to: &mut Device, guard: DeleteGuard) -> SyncOutcome {
            let key = if from == "a" { "a" } else { "b" };
            let outcome = to.ud.sync_merge(file, to.bases.get(key), guard).unwrap();
            if let SyncOutcome::Merged(merged) = &outcome {
                to.bases.insert(key, merged.snapshot.clone());
            }
            outcome
        }

        fn try_sync(from: &Device, to: &mut Device, guard: DeleteGuard) -> SyncOutcome {
            let file = sync_file(from);
            merge_file(&file, from.name, to, guard)
        }

        fn merged(outcome: SyncOutcome) -> SyncMerge {
            match outcome {
                SyncOutcome::Merged(merged) => merged,
                other => panic!("expected a merge, got {other:?}"),
            }
        }

        fn sync(from: &Device, to: &mut Device) -> SyncMerge {
            merged(try_sync(from, to, DeleteGuard::Check))
        }

        /// Sync A→B and B→A, so each has a base for the other.
        fn sync_both(a: &mut Device, b: &mut Device) {
            sync(a, b);
            sync(b, a);
        }

        fn mark_color(ud: &UserData, guid: &str) -> Option<i64> {
            ud.conn
                .query_row(
                    "SELECT ColorIndex FROM UserMark WHERE UserMarkGuid = ?1",
                    [guid],
                    |r| r.get(0),
                )
                .optional()
                .unwrap()
        }

        fn has_note(ud: &UserData, guid: &str) -> bool {
            ud.all_notes(None).unwrap().iter().any(|n| n.guid == guid)
        }

        fn conflict_guids(report: &MergeReport) -> Vec<&str> {
            report.conflicts.iter().map(|c| c.guid.as_str()).collect()
        }

        /// The snapshot without the ranks of tag maps, which are not merged.
        fn rows_but_order(ud: &UserData) -> Snapshot {
            let mut snapshot = ud.snapshot().unwrap();
            for (key, hash) in snapshot.0.iter_mut() {
                if matches!(key, crate::sync_snapshot::RowKey::TagMap(..)) {
                    hash.clear();
                }
            }
            snapshot
        }

        fn add_mark(ud: &mut UserData, chapter: i64) -> String {
            ud.add_mark(&bible(chapter), None, 1, &[verse(1, 0, 3)])
                .unwrap()
        }

        #[test]
        fn favorites_sync_additions_and_removals_without_deleting_study_data() {
            let tmp = tempfile::tempdir().unwrap();
            let (mut a, mut b) = (device(tmp.path(), "a"), device(tmp.path(), "b"));
            let keys = [
                publication_key("w", 0, 20260800),
                publication_key("lff", 2, 0),
            ];
            let mark = add_mark(&mut a.ud, 23);
            for key in &keys {
                a.ud.set_favorite(key, true).unwrap();
            }
            sync_both(&mut a, &mut b);
            assert_eq!(b.ud.favorites().unwrap(), keys);
            b.ud.set_favorite(&keys[0], false).unwrap();
            sync_both(&mut b, &mut a);
            assert_eq!(a.ud.favorites().unwrap(), [keys[1].clone()]);
            assert_eq!(b.ud.favorites().unwrap(), [keys[1].clone()]);
            assert_eq!(mark_color(&a.ud, &mark), Some(1));
            assert_eq!(mark_color(&b.ud, &mark), Some(1));
            a.ud.set_favorite(&keys[0], true).unwrap();
            sync_both(&mut a, &mut b);
            assert_eq!(
                b.ud.favorites().unwrap(),
                [keys[1].clone(), keys[0].clone()]
            );
        }

        #[test]
        fn new_mark_arrives() {
            let tmp = tempfile::tempdir().unwrap();
            let (mut a, mut b) = (device(tmp.path(), "a"), device(tmp.path(), "b"));
            // Without a base, then with one.
            let first = add_mark(&mut a.ud, 1);
            assert_eq!(sync(&a, &mut b).report.added, 1);
            let m = add_mark(&mut a.ud, 2);
            let r = sync(&a, &mut b);
            assert_eq!(mark_color(&b.ud, &first), Some(1));
            assert_eq!(mark_color(&b.ud, &m), Some(1));
            assert_eq!((r.report.added, r.deleted), (1, 0));
            assert!(r.report.conflicts.is_empty());
        }

        #[test]
        fn deletion_carries_over_and_stays() {
            let tmp = tempfile::tempdir().unwrap();
            let (mut a, mut b) = (device(tmp.path(), "a"), device(tmp.path(), "b"));
            let m = add_mark(&mut a.ud, 1);
            let keep = add_mark(&mut a.ud, 2);
            sync_both(&mut a, &mut b);
            b.ud.delete_mark(&m).unwrap();
            let r = sync(&b, &mut a);
            assert_eq!(r.deleted, 1);
            assert_eq!(mark_color(&a.ud, &m), None);
            assert_eq!(mark_color(&a.ud, &keep), Some(1));
            // Another round brings nothing back.
            sync(&a, &mut b);
            sync(&b, &mut a);
            assert_eq!(mark_color(&a.ud, &m), None);
            assert_eq!(mark_color(&b.ud, &m), None);
            assert_eq!(count(&a.ud, "UserMark"), 1);
            assert_eq!(count(&b.ud, "UserMark"), 1);
        }

        #[test]
        fn local_delete_not_resurrected() {
            let tmp = tempfile::tempdir().unwrap();
            let (mut a, mut b) = (device(tmp.path(), "a"), device(tmp.path(), "b"));
            let x = add_mark(&mut a.ud, 1);
            let y = add_mark(&mut a.ud, 2);
            sync_both(&mut a, &mut b);
            a.ud.delete_mark(&x).unwrap();
            // B has not seen the deletion; its file still holds X.
            b.ud.set_mark_color(&y, 4).unwrap();
            let r = sync(&b, &mut a);
            assert_eq!(mark_color(&a.ud, &x), None);
            assert_eq!(mark_color(&a.ud, &y), Some(4));
            assert!(r.report.conflicts.is_empty());
            // And B then loses X.
            assert_eq!(sync(&a, &mut b).deleted, 1);
            assert_eq!(mark_color(&b.ud, &x), None);
        }

        #[test]
        fn stale_copy_does_not_overwrite_edit() {
            let tmp = tempfile::tempdir().unwrap();
            let (mut a, mut b) = (device(tmp.path(), "a"), device(tmp.path(), "b"));
            let h = add_mark(&mut a.ud, 1);
            let y = add_mark(&mut a.ud, 2);
            sync_both(&mut a, &mut b);
            a.ud.set_mark_color(&h, 5).unwrap();
            set_db_time(&a.ud, "2026-01-01T10:00:00Z");
            b.ud.set_mark_color(&y, 3).unwrap();
            set_db_time(&b.ud, "2026-01-01T11:00:00Z");
            let r = sync(&b, &mut a);
            assert_eq!(mark_color(&a.ud, &h), Some(5));
            assert_eq!(mark_color(&a.ud, &y), Some(3));
            assert!(r.report.conflicts.is_empty());
            assert_eq!(r.report.updated, 1);
        }

        #[test]
        fn edit_beats_delete_both_ways() {
            let tmp = tempfile::tempdir().unwrap();
            let (mut a, mut b) = (device(tmp.path(), "a"), device(tmp.path(), "b"));
            let n =
                a.ud.save_note(Some(&bible(1)), None, &note_input("N", "text", &["study"]))
                    .unwrap();
            sync_both(&mut a, &mut b);
            edit_note(&mut a.ud, &n, "N", "edited", &["study"]);
            b.ud.delete_note(&n).unwrap();

            let r = sync(&b, &mut a);
            assert_eq!(r.deleted, 0);
            assert_eq!(note(&a.ud, &n).content, "edited");
            assert_eq!(note(&a.ud, &n).tags, vec!["study"]);
            assert_eq!(conflict_guids(&r.report), vec![n.as_str()]);

            let r = sync(&a, &mut b);
            assert_eq!(note(&b.ud, &n).content, "edited");
            assert_eq!(note(&b.ud, &n).tags, vec!["study"]);
            assert_eq!(conflict_guids(&r.report), vec![n.as_str()]);
            // Settled: the next rounds change nothing.
            let r = sync(&b, &mut a);
            assert!(r.report.conflicts.is_empty());
            assert_eq!((r.report.added, r.report.updated, r.deleted), (0, 0, 0));
        }

        #[test]
        fn no_base_never_deletes() {
            let tmp = tempfile::tempdir().unwrap();
            let (mut a, b) = (device(tmp.path(), "a"), device(tmp.path(), "b"));
            add_mark(&mut a.ud, 1);
            a.ud.save_note(Some(&bible(1)), None, &note_input("N", "text", &["study"]))
                .unwrap();
            let before = a.ud.fingerprint().unwrap();
            let r = merged(try_sync(&b, &mut a, DeleteGuard::Apply));
            assert_eq!(r.deleted, 0);
            assert_eq!(a.ud.fingerprint().unwrap(), before);
            assert_eq!(count(&a.ud, "UserMark"), 1);
            assert_eq!(count(&a.ud, "Note"), 1);
        }

        #[test]
        fn no_base_matches_merge_backup() {
            let tmp = tempfile::tempdir().unwrap();
            let mut a = device(tmp.path(), "a");
            let mut b = device(tmp.path(), "b");
            let m = add_mark(&mut b.ud, 1);
            b.ud.set_mark_color(&m, 2).unwrap();
            b.ud.save_note(Some(&bible(1)), None, &note_input("Local", "L", &["x"]))
                .unwrap();
            // C is a copy of B.
            fs::create_dir_all(tmp.path().join("c")).unwrap();
            fs::copy(b.ud.path(), tmp.path().join("c").join(DB_NAME)).unwrap();
            let mut c = device(tmp.path(), "c");
            add_mark(&mut a.ud, 2);
            a.ud.save_note(Some(&bible(2)), None, &note_input("Remote", "R", &["y"]))
                .unwrap();
            let file = sync_file(&a);
            let synced = merged(b.ud.sync_merge(&file, None, DeleteGuard::Check).unwrap());
            let manual = c.ud.merge_backup(&file).unwrap();
            assert_eq!(synced.report, manual);
            assert_eq!(synced.deleted, 0);
            assert_eq!(b.ud.snapshot().unwrap(), c.ud.snapshot().unwrap());
            assert_eq!(synced.snapshot, a.ud.snapshot().unwrap());
            assert!(b.ud.path().with_extension("db.before-restore").exists());
        }

        /// A has 20 highlights synced to B, and B deleted all of them.
        fn mass_delete(tmp: &Path) -> (Device, Device) {
            let (mut a, mut b) = (device(tmp, "a"), device(tmp, "b"));
            let marks: Vec<String> = (1..=20).map(|c| add_mark(&mut a.ud, c)).collect();
            sync_both(&mut a, &mut b);
            for m in &marks {
                b.ud.delete_mark(m).unwrap();
            }
            (a, b)
        }

        #[test]
        fn mass_delete_needs_confirmation() {
            let tmp = tempfile::tempdir().unwrap();
            let (mut a, b) = mass_delete(&tmp.path().join("check"));
            let safety = a.ud.path().with_extension("db.before-restore");
            fs::remove_file(&safety).unwrap();
            let before = a.ud.fingerprint().unwrap();
            let base = a.bases["b"].clone();
            let outcome = try_sync(&b, &mut a, DeleteGuard::Check);
            assert_eq!(
                outcome,
                SyncOutcome::NeedsConfirmation {
                    deletions: 20,
                    total: 20
                }
            );
            assert_eq!(a.ud.fingerprint().unwrap(), before);
            assert!(!safety.exists());
            assert_eq!(a.bases["b"], base);

            let (mut a, b) = mass_delete(&tmp.path().join("apply"));
            let r = merged(try_sync(&b, &mut a, DeleteGuard::Apply));
            assert_eq!(r.deleted, 20);
            assert_eq!(count(&a.ud, "UserMark"), 0);
            assert_eq!(count(&a.ud, "BlockRange"), 0);

            let (mut a, b) = mass_delete(&tmp.path().join("skip"));
            let r = merged(try_sync(&b, &mut a, DeleteGuard::Skip));
            assert_eq!(r.deleted, 0);
            assert_eq!(count(&a.ud, "UserMark"), 20);
            // The returned snapshot is the new base: the next merge of the
            // same file deletes nothing and asks nothing.
            let r = sync(&b, &mut a);
            assert_eq!(r.deleted, 0);
            assert_eq!(count(&a.ud, "UserMark"), 20);
        }

        /// A with one highlight, 30 answers and 3 bookmarks, synced with B.
        fn answers_and_bookmarks(tmp: &Path) -> (Device, Device) {
            let (mut a, mut b) = (device(tmp, "a"), device(tmp, "b"));
            add_mark(&mut a.ud, 1);
            for n in 0..30 {
                a.ud.save_answer(&watchtower(), None, &format!("tt{n}"), "an answer")
                    .unwrap();
            }
            for slot in 0..3 {
                put_bookmark(&a.ud, slot, slot + 1, "Mark");
            }
            sync_both(&mut a, &mut b);
            (a, b)
        }

        #[test]
        fn a_reset_peer_needs_confirmation_for_every_kind() {
            let tmp = tempfile::tempdir().unwrap();
            let (mut a, b) = answers_and_bookmarks(tmp.path());
            // B is reset to an empty database but keeps its bases: A's rows
            // stay deleted there, and its file now lacks all of them.
            let mut b = Device {
                ud: {
                    let path = b.ud.path().to_owned();
                    drop(b.ud);
                    fs::remove_file(&path).unwrap();
                    UserData::open(&path).unwrap()
                },
                ..b
            };
            let r = sync(&a, &mut b);
            assert_eq!((r.report.added, r.deleted), (0, 0));
            let safety = a.ud.path().with_extension("db.before-restore");
            fs::remove_file(&safety).unwrap();
            let before = a.ud.fingerprint().unwrap();
            let outcome = try_sync(&b, &mut a, DeleteGuard::Check);
            assert_eq!(
                outcome,
                SyncOutcome::NeedsConfirmation {
                    deletions: 34,
                    total: 34
                }
            );
            assert_eq!(a.ud.fingerprint().unwrap(), before);
            assert!(!safety.exists());
        }

        #[test]
        fn wiping_answers_or_bookmarks_needs_confirmation() {
            let tmp = tempfile::tempdir().unwrap();
            // Only answers deleted: the highlight and bookmarks stay.
            let (mut a, mut b) = answers_and_bookmarks(&tmp.path().join("answers"));
            for n in 0..30 {
                b.ud.save_answer(&watchtower(), None, &format!("tt{n}"), "")
                    .unwrap();
            }
            let before = a.ud.fingerprint().unwrap();
            assert_eq!(
                try_sync(&b, &mut a, DeleteGuard::Check),
                SyncOutcome::NeedsConfirmation {
                    deletions: 30,
                    total: 34
                }
            );
            assert_eq!(a.ud.fingerprint().unwrap(), before);
            // Apply deletes them, Skip keeps them.
            let r = merged(try_sync(&b, &mut a, DeleteGuard::Apply));
            assert_eq!(r.deleted, 30);
            assert!(a.ud.answers(&watchtower()).unwrap().is_empty());
            let (mut a, mut b) = answers_and_bookmarks(&tmp.path().join("skip"));
            for n in 0..30 {
                b.ud.save_answer(&watchtower(), None, &format!("tt{n}"), "")
                    .unwrap();
            }
            let r = merged(try_sync(&b, &mut a, DeleteGuard::Skip));
            assert_eq!(r.deleted, 0);
            assert_eq!(a.ud.answers(&watchtower()).unwrap().len(), 30);

            // Tags and their maps: 6 of 6 rows.
            let (mut a, mut b) = (
                device(&tmp.path().join("tags"), "a"),
                device(&tmp.path().join("tags"), "b"),
            );
            let notes: Vec<String> = (1..=3)
                .map(|c| {
                    a.ud.save_note(
                        Some(&bible(c)),
                        None,
                        &note_input("N", "n", &[&format!("t{c}")]),
                    )
                    .unwrap()
                })
                .collect();
            sync_both(&mut a, &mut b);
            b.ud.conn
                .execute_batch("DELETE FROM TagMap; DELETE FROM Tag;")
                .unwrap();
            assert_eq!(
                try_sync(&b, &mut a, DeleteGuard::Check),
                SyncOutcome::NeedsConfirmation {
                    deletions: 6,
                    total: 9
                }
            );
            assert_eq!(note(&a.ud, &notes[0]).tags, vec!["t1"]);
        }

        #[test]
        fn a_few_deleted_answers_need_no_confirmation() {
            let tmp = tempfile::tempdir().unwrap();
            let (mut a, mut b) = answers_and_bookmarks(tmp.path());
            for n in 0..5 {
                b.ud.save_answer(&watchtower(), None, &format!("tt{n}"), "")
                    .unwrap();
            }
            let r = sync(&b, &mut a);
            assert_eq!(r.deleted, 5);
            assert_eq!(a.ud.answers(&watchtower()).unwrap().len(), 25);
        }

        #[test]
        fn few_deletions_need_no_confirmation() {
            let tmp = tempfile::tempdir().unwrap();
            let (mut a, mut b) = (device(tmp.path(), "a"), device(tmp.path(), "b"));
            // 5 of 6 rows: more than half, but not more than 5.
            let marks: Vec<String> = (1..=6).map(|c| add_mark(&mut a.ud, c)).collect();
            sync_both(&mut a, &mut b);
            for m in &marks[..5] {
                b.ud.delete_mark(m).unwrap();
            }
            assert_eq!(sync(&b, &mut a).deleted, 5);
            assert_eq!(count(&a.ud, "UserMark"), 1);
        }

        #[test]
        fn both_changed_differently_settles_the_same_way_and_reports() {
            let tmp = tempfile::tempdir().unwrap();
            let (mut a, mut b) = (device(tmp.path(), "a"), device(tmp.path(), "b"));
            let m = add_mark(&mut a.ud, 1);
            let n =
                a.ud.save_note(Some(&bible(1)), None, &note_input("N", "text", &[]))
                    .unwrap();
            sync_both(&mut a, &mut b);

            a.ud.set_mark_color(&m, 2).unwrap();
            edit_note(&mut a.ud, &n, "N", "from A", &[]);
            set_note_time(&a.ud, &n, "2026-01-01T12:00:00Z");
            set_db_time(&a.ud, "2026-01-01T10:00:00Z");
            b.ud.set_mark_color(&m, 3).unwrap();
            edit_note(&mut b.ud, &n, "N", "from B", &[]);
            set_note_time(&b.ud, &n, "2026-01-01T09:00:00Z");
            set_db_time(&b.ud, "2026-01-01T11:00:00Z");
            // The mark has no clock of its own: the greater content hash
            // wins on both computers, whichever database is newer.
            let key = RowKey::Mark(m.clone());
            let hash_a = a.ud.snapshot().unwrap().0[&key].clone();
            let hash_b = b.ud.snapshot().unwrap().0[&key].clone();
            let (color, a_won) = if hash_b > hash_a {
                (3, false)
            } else {
                (2, true)
            };

            let r = sync(&b, &mut a);
            // The mark goes to the greater hash, the note its own time.
            assert_eq!(mark_color(&a.ud, &m), Some(color));
            assert_eq!(note(&a.ud, &n).content, "from A");
            let mut names = conflict_guids(&r.report);
            names.sort_unstable();
            let mut want = vec![m.as_str(), n.as_str()];
            want.sort_unstable();
            assert_eq!(names, want);
            assert_eq!(r.report.conflicts.len(), 2);

            // B then takes A's note, newer by its time, and the mark by the
            // same hash rule; each lost edit is reported.
            let r = sync(&a, &mut b);
            assert_eq!(note(&b.ud, &n).content, "from A");
            assert_eq!(mark_color(&b.ud, &m), Some(color));
            let mut names = conflict_guids(&r.report);
            names.sort_unstable();
            let mut want = vec![n.as_str()];
            if a_won {
                want.push(m.as_str());
            }
            want.sort_unstable();
            assert_eq!(names, want);
            assert_eq!(a.ud.snapshot().unwrap(), b.ud.snapshot().unwrap());
            // Settled.
            let r = sync(&b, &mut a);
            assert!(r.report.conflicts.is_empty(), "{:?}", r.report.conflicts);
            assert_eq!((r.report.added, r.report.updated, r.deleted), (0, 0, 0));
        }

        #[test]
        fn rows_new_on_both_sides_settle_the_same_way() {
            let tmp = tempfile::tempdir().unwrap();
            let (mut a, mut b) = (device(tmp.path(), "a"), device(tmp.path(), "b"));
            add_mark(&mut a.ud, 1);
            sync_both(&mut a, &mut b);
            // Both set bookmark slot 0 of the same publication, differently,
            // with the same database clock.
            put_bookmark(&a.ud, 0, 2, "From A");
            put_bookmark(&b.ud, 0, 3, "From B");
            set_db_time(&a.ud, "2026-01-01T10:00:00Z");
            set_db_time(&b.ud, "2026-01-01T10:00:00Z");
            let r1 = sync(&b, &mut a);
            let r2 = sync(&a, &mut b);
            assert_eq!(a.ud.snapshot().unwrap(), b.ud.snapshot().unwrap());
            assert!(
                r1.report.conflicts.iter().any(|c| c.table == "Bookmark")
                    || r2.report.conflicts.iter().any(|c| c.table == "Bookmark")
            );
            let r = sync(&b, &mut a);
            assert_eq!((r.report.added, r.report.updated, r.deleted), (0, 0, 0));
        }

        #[test]
        fn notes_changed_at_the_same_time_settle_by_hash() {
            let tmp = tempfile::tempdir().unwrap();
            let (mut a, mut b) = (device(tmp.path(), "a"), device(tmp.path(), "b"));
            let n =
                a.ud.save_note(Some(&bible(1)), None, &note_input("N", "text", &[]))
                    .unwrap();
            sync_both(&mut a, &mut b);
            edit_note(&mut a.ud, &n, "N", "from A", &[]);
            edit_note(&mut b.ud, &n, "N", "from B", &[]);
            set_note_time(&a.ud, &n, "2026-01-01T12:00:00Z");
            set_note_time(&b.ud, &n, "2026-01-01T12:00:00Z");
            let key = RowKey::Note(n.clone());
            let hash_a = a.ud.snapshot().unwrap().0[&key].clone();
            let hash_b = b.ud.snapshot().unwrap().0[&key].clone();
            let content = if hash_b > hash_a { "from B" } else { "from A" };
            let r1 = sync(&b, &mut a);
            let r2 = sync(&a, &mut b);
            assert_eq!(note(&a.ud, &n).content, content);
            assert_eq!(note(&b.ud, &n).content, content);
            assert!(!r1.report.conflicts.is_empty() || !r2.report.conflicts.is_empty());
            assert_eq!(a.ud.snapshot().unwrap(), b.ud.snapshot().unwrap());
        }

        #[test]
        fn tags_and_bookmarks_and_input_fields() {
            let tmp = tempfile::tempdir().unwrap();
            let (mut a, mut b) = (device(tmp.path(), "a"), device(tmp.path(), "b"));
            let n1 =
                a.ud.save_note(Some(&bible(1)), None, &note_input("One", "1", &["a", "b"]))
                    .unwrap();
            let n2 =
                a.ud.save_note(Some(&bible(2)), None, &note_input("Two", "2", &["a"]))
                    .unwrap();
            put_bookmark(&a.ud, 0, 1, "One");
            put_bookmark(&a.ud, 1, 2, "Two");
            a.ud.save_answer(&watchtower(), None, "tt1", "first")
                .unwrap();
            a.ud.save_answer(&watchtower(), None, "tt2", "second")
                .unwrap();
            sync_both(&mut a, &mut b);

            // B: tag `b` leaves note 1 and is deleted, note 2 gains tag `c`.
            edit_note(&mut b.ud, &n1, "One", "1", &["a"]);
            b.ud.conn
                .execute("DELETE FROM Tag WHERE Name = 'b'", [])
                .unwrap();
            edit_note(&mut b.ud, &n2, "Two", "2", &["a", "c"]);
            b.ud.conn
                .execute("DELETE FROM Bookmark WHERE Slot = 0", [])
                .unwrap();
            put_bookmark(&b.ud, 1, 3, "Two moved");
            b.ud.save_answer(&watchtower(), None, "tt1", "").unwrap();
            b.ud.save_answer(&watchtower(), None, "tt2", "changed")
                .unwrap();

            let r = sync(&b, &mut a);
            assert!(r.report.conflicts.is_empty(), "{:?}", r.report.conflicts);
            // The tag map, the tag, the bookmark and the answer.
            assert_eq!(r.deleted, 4);
            assert_eq!(note(&a.ud, &n1).tags, vec!["a"]);
            assert_eq!(note(&a.ud, &n2).tags, vec!["a", "c"]);
            let names: Vec<String> = a.ud.tags().unwrap().into_iter().map(|t| t.name).collect();
            assert_eq!(names, vec!["a", "c"]);
            assert_eq!(bookmark_slots(&a.ud), vec![(1, 3, "Two moved".to_string())]);
            let answers = a.ud.answers(&watchtower()).unwrap();
            assert_eq!(answers.len(), 1);
            assert_eq!(answers["tt2"], "changed");
            assert_foreign_keys_hold(&a.ud);
            assert_eq!(a.ud.snapshot().unwrap(), b.ud.snapshot().unwrap());
        }

        #[test]
        fn deleting_a_note_and_a_mark_keeps_the_rest_consistent() {
            let tmp = tempfile::tempdir().unwrap();
            let (mut a, mut b) = (device(tmp.path(), "a"), device(tmp.path(), "b"));
            let m = add_mark(&mut a.ud, 1);
            let attached =
                a.ud.save_note(
                    Some(&bible(1)),
                    None,
                    &NoteInput {
                        mark_guid: Some(m.clone()),
                        ..note_input("On the mark", "x", &["t"])
                    },
                )
                .unwrap();
            let gone =
                a.ud.save_note(Some(&bible(2)), None, &note_input("Gone", "y", &["t"]))
                    .unwrap();
            sync_both(&mut a, &mut b);
            b.ud.delete_mark(&m).unwrap();
            b.ud.delete_note(&gone).unwrap();
            let r = sync(&b, &mut a);
            assert!(r.report.conflicts.is_empty(), "{:?}", r.report.conflicts);
            assert_eq!(mark_color(&a.ud, &m), None);
            assert!(!has_note(&a.ud, &gone));
            // The note on the deleted highlight stays, without it.
            assert_eq!(note(&a.ud, &attached).mark_guid, None);
            assert_eq!(note(&a.ud, &attached).tags, vec!["t"]);
            assert_foreign_keys_hold(&a.ud);
            assert_eq!(a.ud.snapshot().unwrap(), b.ud.snapshot().unwrap());
        }

        #[test]
        fn sync_no_op_changes_nothing() {
            let tmp = tempfile::tempdir().unwrap();
            let (mut a, mut b) = (device(tmp.path(), "a"), device(tmp.path(), "b"));
            let m = add_mark(&mut a.ud, 1);
            a.ud.save_note(
                Some(&bible(1)),
                None,
                &NoteInput {
                    mark_guid: Some(m),
                    ..note_input("N", "text", &["x", "y"])
                },
            )
            .unwrap();
            a.ud.save_note(Some(&watchtower()), None, &note_input("W", "w", &["y"]))
                .unwrap();
            put_bookmark(&a.ud, 0, 1, "One");
            a.ud.save_answer(&watchtower(), None, "tt1", "an answer")
                .unwrap();
            sync_both(&mut a, &mut b);

            let assert_no_op = |r: &SyncMerge| {
                assert_eq!(r.report.added, 0);
                assert_eq!(r.report.updated, 0);
                assert!(r.report.conflicts.is_empty(), "{:?}", r.report.conflicts);
                assert_eq!(r.deleted, 0);
                assert!(r.report.unchanged > 0);
            };
            // A file identical to the base.
            let before = b.ud.fingerprint().unwrap();
            let clock = db_time(&b.ud);
            let file = sync_file(&a);
            assert_no_op(&merged(merge_file(&file, "a", &mut b, DeleteGuard::Check)));
            assert_eq!(b.ud.fingerprint().unwrap(), before);
            // The same file merged twice.
            assert_no_op(&merged(merge_file(&file, "a", &mut b, DeleteGuard::Check)));
            assert_eq!(b.ud.fingerprint().unwrap(), before);
            assert_eq!(db_time(&b.ud), clock);
            // And the other way.
            let before = a.ud.fingerprint().unwrap();
            assert_no_op(&sync(&b, &mut a));
            assert_eq!(a.ud.fingerprint().unwrap(), before);
        }

        #[test]
        fn overlapping_rows_on_both_sides() {
            let tmp = tempfile::tempdir().unwrap();
            let (mut a, mut b) = (device(tmp.path(), "a"), device(tmp.path(), "b"));
            // Both sides hold tag `shared` and answers on the same page.
            let na =
                a.ud.save_note(Some(&bible(1)), None, &note_input("A", "a", &["shared"]))
                    .unwrap();
            a.ud.save_answer(&watchtower(), None, "tt1", "same")
                .unwrap();
            a.ud.save_answer(&watchtower(), None, "tt2", "from A")
                .unwrap();
            let nb =
                b.ud.save_note(Some(&bible(2)), None, &note_input("B", "b", &["shared"]))
                    .unwrap();
            b.ud.save_answer(&watchtower(), None, "tt1", "same")
                .unwrap();
            b.ud.save_answer(&watchtower(), None, "tt3", "from B")
                .unwrap();
            sync_both(&mut a, &mut b);
            sync(&a, &mut b);
            // The same rows; the order of `shared` may differ.
            assert_eq!(rows_but_order(&a.ud), rows_but_order(&b.ud));

            // B takes its note out of `shared`, A edits the shared answer.
            edit_note(&mut b.ud, &nb, "B", "b", &[]);
            a.ud.save_answer(&watchtower(), None, "tt1", "edited")
                .unwrap();
            // Both add the same new answer and tag a note `fresh`.
            a.ud.save_answer(&watchtower(), None, "tt4", "new").unwrap();
            b.ud.save_answer(&watchtower(), None, "tt4", "new").unwrap();
            edit_note(&mut a.ud, &na, "A", "a", &["fresh", "shared"]);
            edit_note(&mut b.ud, &nb, "B", "b", &["fresh"]);

            let r = sync(&b, &mut a);
            assert!(r.report.conflicts.is_empty(), "{:?}", r.report.conflicts);
            assert_eq!(r.deleted, 1);
            assert_eq!(note(&a.ud, &na).tags, vec!["fresh", "shared"]);
            assert_eq!(note(&a.ud, &nb).tags, vec!["fresh"]);
            let answers = a.ud.answers(&watchtower()).unwrap();
            assert_eq!(answers["tt1"], "edited");
            assert_eq!(answers["tt4"], "new");

            let r = sync(&a, &mut b);
            assert!(r.report.conflicts.is_empty(), "{:?}", r.report.conflicts);
            assert_eq!(b.ud.answers(&watchtower()).unwrap(), answers);
            assert_eq!(note(&b.ud, &na).tags, vec!["fresh", "shared"]);
            assert_eq!(note(&b.ud, &nb).tags, vec!["fresh"]);
        }

        #[test]
        fn tag_order_differences_are_not_conflicts() {
            let tmp = tempfile::tempdir().unwrap();
            let (mut a, mut b) = (device(tmp.path(), "a"), device(tmp.path(), "b"));
            let n1 =
                a.ud.save_note(Some(&bible(1)), None, &note_input("1", "1", &["order"]))
                    .unwrap();
            let n2 =
                a.ud.save_note(Some(&bible(2)), None, &note_input("2", "2", &["order"]))
                    .unwrap();
            sync_both(&mut a, &mut b);
            // B moves note 1 to the end of `order`: [2, 1] there, [1, 2] here.
            edit_note(&mut b.ud, &n1, "1", "1", &[]);
            edit_note(&mut b.ud, &n1, "1", "1", &["order"]);
            edit_note(&mut a.ud, &n2, "2", "edited", &["order"]);
            for _ in 0..2 {
                let r = sync(&b, &mut a);
                assert!(r.report.conflicts.is_empty(), "{:?}", r.report.conflicts);
                assert_eq!(r.deleted, 0);
                let r = sync(&a, &mut b);
                assert!(r.report.conflicts.is_empty(), "{:?}", r.report.conflicts);
                assert_eq!(r.deleted, 0);
            }
            for ud in [&a.ud, &b.ud] {
                assert_eq!(note(ud, &n1).tags, vec!["order"]);
                assert_eq!(note(ud, &n2).tags, vec!["order"]);
                assert_eq!(note(ud, &n2).content, "edited");
            }
        }

        #[test]
        fn deletion_spares_an_invisible_twin() {
            let tmp = tempfile::tempdir().unwrap();
            let (mut a, mut b) = (device(tmp.path(), "a"), device(tmp.path(), "b"));
            put_bookmark(&a.ud, 0, 1, "Seen");
            sync_both(&mut a, &mut b);
            // A second Bible location that normalizes to the same key (no
            // book there, book 0 here), with a bookmark in the same slot:
            // the snapshot sees only the first.
            a.ud.conn
                .execute_batch(
                    "INSERT INTO Location (BookNumber, KeySymbol, MepsLanguage, Type)
                     VALUES (0, 'nwtsty', 2, 1);
                     INSERT INTO Bookmark (LocationId, PublicationLocationId, Slot, Title,
                                           BlockType)
                     SELECT (SELECT LocationId FROM Location WHERE ChapterNumber = 1),
                            last_insert_rowid(), 0, 'Twin', 0;",
                )
                .unwrap();
            b.ud.conn.execute("DELETE FROM Bookmark", []).unwrap();
            let r = sync(&b, &mut a);
            assert_eq!(r.deleted, 1);
            let titles: Vec<String> =
                a.ud.conn
                    .prepare("SELECT Title FROM Bookmark")
                    .unwrap()
                    .query_map([], |r| r.get(0))
                    .unwrap()
                    .map(|t| t.unwrap())
                    .collect();
            assert_eq!(titles, vec!["Twin"]);
        }

        #[test]
        fn a_deleted_answer_goes_from_every_sibling_location() {
            let tmp = tempfile::tempdir().unwrap();
            let (mut a, mut b) = (device(tmp.path(), "a"), device(tmp.path(), "b"));
            a.ud.save_answer(&watchtower(), None, "tt1", "shown")
                .unwrap();
            sync_both(&mut a, &mut b);
            // A also has the field on a later location of the page without
            // a language, as JW Library writes it: hidden behind the first.
            a.ud.conn
                .execute_batch(
                    "INSERT INTO Location (DocumentId, IssueTagNumber, KeySymbol, MepsLanguage,
                                           Type)
                     VALUES (2026520, 20260800, 'w', NULL, 0);
                     INSERT INTO InputField (LocationId, TextTag, Value)
                     VALUES (last_insert_rowid(), 'tt1', 'hidden');",
                )
                .unwrap();
            b.ud.save_answer(&watchtower(), None, "tt1", "").unwrap();
            let r = sync(&b, &mut a);
            assert_eq!(r.deleted, 1);
            assert_eq!(count(&a.ud, "InputField"), 0);
            assert!(a.ud.answers(&watchtower()).unwrap().is_empty());
            let snapshot = a.ud.snapshot().unwrap();
            assert!(
                !snapshot
                    .0
                    .keys()
                    .any(|k| matches!(k, crate::sync_snapshot::RowKey::InputField(..)))
            );
        }

        #[test]
        fn a_tag_used_here_survives_its_deletion_there() {
            let tmp = tempfile::tempdir().unwrap();
            let (mut a, mut b) = (device(tmp.path(), "a"), device(tmp.path(), "b"));
            a.ud.save_note(Some(&bible(1)), None, &note_input("1", "1", &["t"]))
                .unwrap();
            let n2 =
                a.ud.save_note(Some(&bible(2)), None, &note_input("2", "2", &[]))
                    .unwrap();
            sync_both(&mut a, &mut b);
            // B deletes tag `t`; A gives it to another note meanwhile.
            b.ud.conn
                .execute_batch(
                    "DELETE FROM TagMap WHERE TagId = (SELECT TagId FROM Tag WHERE Name = 't');
                     DELETE FROM Tag WHERE Name = 't';",
                )
                .unwrap();
            edit_note(&mut a.ud, &n2, "2", "2", &["t"]);
            let r = sync(&b, &mut a);
            assert_eq!(r.deleted, 0);
            assert_eq!(r.report.conflicts.len(), 1, "{:?}", r.report.conflicts);
            assert_eq!(r.report.conflicts[0].table, "Tag");
            assert_eq!(a.ud.tags().unwrap()[0].notes, 2);
            // B gets the tag back with both notes.
            let r = sync(&a, &mut b);
            assert_eq!(r.report.conflicts.len(), 1, "{:?}", r.report.conflicts);
            assert_eq!(b.ud.tags().unwrap()[0].notes, 2);
            assert_eq!(a.ud.snapshot().unwrap(), b.ud.snapshot().unwrap());
        }

        #[test]
        fn deleted_rows_are_never_added_back_by_the_same_merge() {
            let tmp = tempfile::tempdir().unwrap();
            let (mut a, mut b) = (device(tmp.path(), "a"), device(tmp.path(), "b"));
            let old: Vec<String> = (1..=3).map(|c| add_mark(&mut a.ud, c)).collect();
            sync_both(&mut a, &mut b);
            // B replaces the highlights with new ones on the same verses.
            for m in &old {
                b.ud.delete_mark(m).unwrap();
            }
            let new: Vec<String> = (1..=3)
                .map(|c| {
                    b.ud.add_mark(&bible(c), None, 4, &[verse(1, 0, 3)])
                        .unwrap()
                })
                .collect();
            let r = sync(&b, &mut a);
            assert_eq!((r.deleted, r.report.added), (3, 3));
            for m in &old {
                assert_eq!(mark_color(&a.ud, m), None);
            }
            for m in &new {
                assert_eq!(mark_color(&a.ud, m), Some(4));
            }
            assert_eq!(a.ud.snapshot().unwrap(), r.snapshot);
        }
    }
}
