//! Personal study data — highlights, notes, tags and bookmarks — stored in a
//! database with the layout of a JW Library `userData.db`, and `.jwlibrary`
//! backups of it. Table semantics are in docs/FORMAT.md ("User data").

use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use rusqlite::{Connection, OpenFlags, OptionalExtension, Transaction, params};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use zip::ZipArchive;
use zip::write::SimpleFileOptions;

use crate::{Error, Result};

/// File name of the database, inside the library root and inside backups.
pub const DB_NAME: &str = "userData.db";
/// Schema version of a database created here (kept in `PRAGMA user_version`).
pub const SCHEMA_VERSION: i64 = 16;
/// Device name written into backups.
pub const DEVICE_NAME: &str = "Pergament";

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
const TAG_USER: i64 = 1;

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

/// The parts of a backup's `manifest.json` that matter here.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BackupManifest {
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
        tx.execute(
            "DELETE FROM TagMap WHERE NoteId IN (SELECT NoteId FROM Note WHERE Guid = ?1)",
            [guid],
        )?;
        tx.execute("DELETE FROM Note WHERE Guid = ?1", [guid])?;
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

    /// Write a `.jwlibrary` backup of the current data to `dest`.
    pub fn export_backup(&self, dest: &Path) -> Result<()> {
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
                device_name: DEVICE_NAME.into(),
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
            zip.finish()?;
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
}

/// Copy the database out of a backup into `dest` and return the manifest.
fn unpack_backup(src: &Path, dest: &Path) -> Result<BackupManifest> {
    let mut zip = ZipArchive::new(File::open(src)?)?;
    let manifest: BackupManifest = {
        let entry = zip
            .by_name(MANIFEST)
            .map_err(|_| Error::MissingEntry(MANIFEST.into()))?;
        let mut buf = Vec::new();
        entry.take(MAX_MANIFEST + 1).read_to_end(&mut buf)?;
        if buf.len() as u64 > MAX_MANIFEST {
            return Err(Error::TooLarge(MANIFEST.into()));
        }
        serde_json::from_slice(&buf)?
    };
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

/// Accept only an intact SQLite database with the user data tables.
fn check_database(path: &Path) -> Result<()> {
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

fn set_note_tags(tx: &Transaction<'_>, note: i64, tags: &[String]) -> Result<()> {
    tx.execute("DELETE FROM TagMap WHERE NoteId = ?1", [note])?;
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
}
