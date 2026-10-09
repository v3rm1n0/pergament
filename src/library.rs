//! The local library: imported publications plus a SQLite index.
//!
//! Layout below the root (default `$XDG_DATA_HOME/pergament`):
//! `index.sqlite` and `publications/<symbol>_<lang>[_<issue>]/` holding the
//! unpacked `contents` of each publication, and `media/` with downloaded
//! videos and audio.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{Connection, OptionalExtension, params};

use crate::jwpub::{self, JwPub, Limits, PublicationInfo};
use crate::links::MediaKind;
use crate::{Error, Result};

const PUBLICATIONS: &str = "publications";
const MEDIA: &str = "media";

/// An imported publication as recorded in the index.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Entry {
    pub symbol: String,
    pub meps_language: i64,
    pub issue_tag: String,
    pub year: i64,
    pub title: String,
    pub short_title: Option<String>,
    pub publication_type: Option<String>,
    /// Directory name below `publications/`.
    pub dir_name: String,
    pub db_file: String,
    pub contents_hash: String,
    pub imported_at: i64,
    /// Language code from the file name, e.g. `X`; `None` for entries
    /// imported before this was recorded.
    pub lang_code: Option<String>,
}

#[derive(Debug)]
pub struct Library {
    root: PathBuf,
    index: Connection,
    limits: Limits,
}

/// The app's folder below `base` (`$XDG_DATA_HOME` or `$XDG_CACHE_HOME`).
/// A folder from before the rename (`jwlinux`) is moved there once.
pub fn app_dir(base: &Path) -> PathBuf {
    let dir = base.join("pergament");
    let old = base.join("jwlinux");
    if !dir.exists() && old.is_dir() {
        // If the move fails the old folder stays and the app starts empty.
        let _ = fs::rename(&old, &dir);
    }
    dir
}

impl Library {
    /// Open the library at `$XDG_DATA_HOME/pergament`.
    pub fn open_default() -> Result<Self> {
        Self::open(app_dir(&dirs::data_dir().ok_or(Error::NoDataDir)?))
    }

    pub fn open(root: impl Into<PathBuf>) -> Result<Self> {
        let root = root.into();
        fs::create_dir_all(root.join(PUBLICATIONS))?;
        let index = Connection::open(root.join("index.sqlite"))?;
        index.execute_batch(
            "CREATE TABLE IF NOT EXISTS publication (
                 id               INTEGER PRIMARY KEY,
                 symbol           TEXT    NOT NULL,
                 meps_language    INTEGER NOT NULL,
                 issue_tag        TEXT    NOT NULL,
                 year             INTEGER NOT NULL,
                 title            TEXT    NOT NULL,
                 short_title      TEXT,
                 publication_type TEXT,
                 dir_name         TEXT    NOT NULL UNIQUE,
                 db_file          TEXT    NOT NULL,
                 contents_hash    TEXT    NOT NULL,
                 imported_at      INTEGER NOT NULL,
                 UNIQUE (symbol, meps_language, issue_tag)
             );
             CREATE TABLE IF NOT EXISTS media (
                 id            INTEGER PRIMARY KEY,
                 key           TEXT    NOT NULL,
                 lang_code     TEXT    NOT NULL,
                 title         TEXT    NOT NULL,
                 kind          TEXT    NOT NULL,
                 label         TEXT    NOT NULL,
                 file_name     TEXT    NOT NULL UNIQUE,
                 subtitle_file TEXT,
                 size          INTEGER NOT NULL,
                 duration      REAL,
                 image         TEXT,
                 downloaded_at INTEGER NOT NULL,
                 UNIQUE (key, lang_code)
             );",
        )?;
        // Added after the first release of the index.
        let has_lang: bool = index
            .prepare("SELECT 1 FROM pragma_table_info('publication') WHERE name = 'lang_code'")?
            .exists([])?;
        if !has_lang {
            index.execute_batch("ALTER TABLE publication ADD COLUMN lang_code TEXT;")?;
        }
        Ok(Self {
            root,
            index,
            limits: Limits::default(),
        })
    }

    pub fn with_limits(mut self, limits: Limits) -> Self {
        self.limits = limits;
        self
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Directory holding the unpacked files of `entry`.
    pub fn publication_dir(&self, entry: &Entry) -> PathBuf {
        self.root.join(PUBLICATIONS).join(&entry.dir_name)
    }

    pub fn db_path(&self, entry: &Entry) -> PathBuf {
        self.publication_dir(entry).join(&entry.db_file)
    }

    /// Import a `.jwpub`. An existing entry for the same publication
    /// (symbol, language, issue) is replaced.
    pub fn import(&mut self, path: impl AsRef<Path>) -> Result<Entry> {
        let mut pub_ = JwPub::open_with_limits(path, self.limits)?;
        let db_file = pub_.db_file_name()?.to_owned();

        let pubs_dir = self.root.join(PUBLICATIONS);
        let staging = tempfile::Builder::new()
            .prefix(".import-")
            .tempdir_in(&pubs_dir)?;
        pub_.extract_to(staging.path())?;

        let db_path = staging.path().join(&db_file);
        if !db_path.is_file() {
            return Err(Error::MissingEntry(db_file));
        }
        jwpub::verify_db_hash(&pub_.manifest, &db_path)?;
        let info = jwpub::inspect_db(&db_path)?;

        let entry = Entry {
            dir_name: dir_name(&info)?,
            symbol: info.symbol,
            meps_language: info.meps_language,
            issue_tag: info.issue_tag,
            year: info.year,
            title: info.title,
            short_title: info.short_title,
            publication_type: info.publication_type,
            db_file,
            contents_hash: pub_.manifest.hash.to_ascii_lowercase(),
            imported_at: now(),
            lang_code: lang_code_from_name(&pub_.manifest.name),
        };

        let final_dir = pubs_dir.join(&entry.dir_name);
        let tx = self.index.transaction()?;
        tx.execute(
            "DELETE FROM publication
             WHERE (symbol = ?1 AND meps_language = ?2 AND issue_tag = ?3) OR dir_name = ?4",
            params![
                entry.symbol,
                entry.meps_language,
                entry.issue_tag,
                entry.dir_name
            ],
        )?;
        tx.execute(
            "INSERT INTO publication (symbol, meps_language, issue_tag, year, title, short_title,
                 publication_type, dir_name, db_file, contents_hash, imported_at, lang_code)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            params![
                entry.symbol,
                entry.meps_language,
                entry.issue_tag,
                entry.year,
                entry.title,
                entry.short_title,
                entry.publication_type,
                entry.dir_name,
                entry.db_file,
                entry.contents_hash,
                entry.imported_at,
                entry.lang_code
            ],
        )?;

        // Swap directories, then commit. If anything fails before the commit
        // the index is rolled back and the old directory is restored.
        let old = pubs_dir.join(format!(".old-{}", entry.dir_name));
        if old.exists() {
            fs::remove_dir_all(&old)?;
        }
        let had_old = final_dir.exists();
        if had_old {
            fs::rename(&final_dir, &old)?;
        }
        let staged = staging.keep();
        if let Err(e) = fs::rename(&staged, &final_dir) {
            let _ = fs::remove_dir_all(&staged);
            if had_old {
                let _ = fs::rename(&old, &final_dir);
            }
            return Err(e.into());
        }
        if let Err(e) = tx.commit() {
            let _ = fs::remove_dir_all(&final_dir);
            if had_old {
                let _ = fs::rename(&old, &final_dir);
            }
            return Err(e.into());
        }
        if had_old {
            fs::remove_dir_all(&old)?;
        }
        Ok(entry)
    }

    /// All imported publications, sorted by language, symbol and issue.
    pub fn list(&self) -> Result<Vec<Entry>> {
        let mut stmt = self.index.prepare(&format!(
            "SELECT {COLUMNS} FROM publication ORDER BY meps_language, symbol, issue_tag"
        ))?;
        let rows = stmt.query_map([], row_to_entry)?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Look up a publication by symbol, e.g. `nwtsty` or `wp26`, optionally
    /// restricted to a MEPS language and issue tag.
    pub fn find(
        &self,
        symbol: &str,
        meps_language: Option<i64>,
        issue_tag: Option<&str>,
    ) -> Result<Vec<Entry>> {
        let mut stmt = self.index.prepare(&format!(
            "SELECT {COLUMNS} FROM publication
             WHERE symbol = ?1 AND (?2 IS NULL OR meps_language = ?2)
               AND (?3 IS NULL OR issue_tag = ?3)
             ORDER BY meps_language, issue_tag"
        ))?;
        let rows = stmt.query_map(params![symbol, meps_language, issue_tag], row_to_entry)?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Remove a publication from the index and delete its files.
    pub fn remove(&mut self, entry: &Entry) -> Result<bool> {
        let n = self.index.execute(
            "DELETE FROM publication WHERE dir_name = ?1",
            [&entry.dir_name],
        )?;
        let dir = self.publication_dir(entry);
        if dir.exists() {
            fs::remove_dir_all(dir)?;
        }
        Ok(n > 0)
    }

    pub fn get_by_dir(&self, dir_name: &str) -> Result<Option<Entry>> {
        Ok(self
            .index
            .query_row(
                &format!("SELECT {COLUMNS} FROM publication WHERE dir_name = ?1"),
                [dir_name],
                row_to_entry,
            )
            .optional()?)
    }
}

/// A downloaded recording as recorded in the index.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct MediaEntry {
    pub id: i64,
    /// `naturalKey` of the recording, unique per language.
    pub key: String,
    pub lang_code: String,
    pub title: String,
    pub kind: MediaKind,
    /// Quality such as `480p`; empty for audio.
    pub label: String,
    /// File name below `media/`.
    pub file_name: String,
    /// WebVTT file name below `media/`, if captions were saved.
    pub subtitle_file: Option<String>,
    pub size: i64,
    pub duration: Option<f64>,
    /// Thumbnail path below [`crate::mediator::IMAGE_BASE`].
    pub image: Option<String>,
    pub downloaded_at: i64,
}

const MEDIA_COLUMNS: &str = "id, key, lang_code, title, kind, label, file_name, subtitle_file, \
     size, duration, image, downloaded_at";

fn row_to_media(r: &rusqlite::Row<'_>) -> rusqlite::Result<MediaEntry> {
    let kind: String = r.get(4)?;
    Ok(MediaEntry {
        id: r.get(0)?,
        key: r.get(1)?,
        lang_code: r.get(2)?,
        title: r.get(3)?,
        kind: if kind == "audio" {
            MediaKind::Audio
        } else {
            MediaKind::Video
        },
        label: r.get(5)?,
        file_name: r.get(6)?,
        subtitle_file: r.get(7)?,
        size: r.get(8)?,
        duration: r.get(9)?,
        image: r.get(10)?,
        downloaded_at: r.get(11)?,
    })
}

/// File name stem for a recording: only `[A-Za-z0-9_-]` survive.
pub fn media_stem(key: &str, lang_code: &str, label: &str) -> String {
    let clean = |s: &str| -> String {
        s.chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '-' {
                    c
                } else {
                    '_'
                }
            })
            .collect()
    };
    let label = clean(label);
    if label.is_empty() {
        format!("{}_{}", clean(key), clean(lang_code))
    } else {
        format!("{}_{}_{label}", clean(key), clean(lang_code))
    }
}

impl Library {
    /// Directory holding downloaded recordings.
    pub fn media_dir(&self) -> PathBuf {
        self.root.join(MEDIA)
    }

    /// Record a downloaded recording whose files are already in
    /// [`Library::media_dir`]. An earlier download of the same recording
    /// (another quality) is replaced and its files deleted.
    pub fn add_media(&self, entry: &MediaEntry) -> Result<MediaEntry> {
        let old = self.find_media(&entry.key, &entry.lang_code)?;
        self.index.execute(
            "INSERT INTO media (key, lang_code, title, kind, label, file_name, subtitle_file,
                                size, duration, image, downloaded_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
             ON CONFLICT (key, lang_code) DO UPDATE SET
                title = excluded.title, kind = excluded.kind, label = excluded.label,
                file_name = excluded.file_name, subtitle_file = excluded.subtitle_file,
                size = excluded.size, duration = excluded.duration, image = excluded.image,
                downloaded_at = excluded.downloaded_at",
            params![
                entry.key,
                entry.lang_code,
                entry.title,
                match entry.kind {
                    MediaKind::Audio => "audio",
                    MediaKind::Video => "video",
                },
                entry.label,
                entry.file_name,
                entry.subtitle_file,
                entry.size,
                entry.duration,
                entry.image,
                entry.downloaded_at,
            ],
        )?;
        if let Some(old) = old {
            let keep = |f: &str| f == entry.file_name || entry.subtitle_file.as_deref() == Some(f);
            for f in [Some(old.file_name), old.subtitle_file]
                .into_iter()
                .flatten()
            {
                if !keep(&f) {
                    let _ = fs::remove_file(self.media_dir().join(f));
                }
            }
        }
        self.find_media(&entry.key, &entry.lang_code)?
            .ok_or_else(|| Error::NotFound(entry.key.clone()))
    }

    pub fn find_media(&self, key: &str, lang_code: &str) -> Result<Option<MediaEntry>> {
        Ok(self
            .index
            .query_row(
                &format!("SELECT {MEDIA_COLUMNS} FROM media WHERE key = ?1 AND lang_code = ?2"),
                [key, lang_code],
                row_to_media,
            )
            .optional()?)
    }

    pub fn get_media(&self, id: i64) -> Result<Option<MediaEntry>> {
        Ok(self
            .index
            .query_row(
                &format!("SELECT {MEDIA_COLUMNS} FROM media WHERE id = ?1"),
                [id],
                row_to_media,
            )
            .optional()?)
    }

    /// Downloaded recordings, newest first. Entries whose file has gone
    /// missing are left out.
    pub fn list_media(&self) -> Result<Vec<MediaEntry>> {
        let mut stmt = self.index.prepare(&format!(
            "SELECT {MEDIA_COLUMNS} FROM media ORDER BY downloaded_at DESC, id DESC"
        ))?;
        let dir = self.media_dir();
        let rows = stmt.query_map([], row_to_media)?;
        Ok(rows
            .collect::<rusqlite::Result<Vec<_>>>()?
            .into_iter()
            .filter(|m| dir.join(&m.file_name).is_file())
            .collect())
    }

    /// Delete a downloaded recording and its files.
    pub fn remove_media(&self, id: i64) -> Result<bool> {
        let Some(entry) = self.get_media(id)? else {
            return Ok(false);
        };
        self.index
            .execute("DELETE FROM media WHERE id = ?1", [id])?;
        for f in [Some(entry.file_name), entry.subtitle_file]
            .into_iter()
            .flatten()
        {
            let _ = fs::remove_file(self.media_dir().join(f));
        }
        Ok(true)
    }
}

const COLUMNS: &str = "symbol, meps_language, issue_tag, year, title, short_title, \
     publication_type, dir_name, db_file, contents_hash, imported_at, lang_code";

fn row_to_entry(r: &rusqlite::Row<'_>) -> rusqlite::Result<Entry> {
    Ok(Entry {
        symbol: r.get(0)?,
        meps_language: r.get(1)?,
        issue_tag: r.get(2)?,
        year: r.get(3)?,
        title: r.get(4)?,
        short_title: r.get(5)?,
        publication_type: r.get(6)?,
        dir_name: r.get(7)?,
        db_file: r.get(8)?,
        contents_hash: r.get(9)?,
        imported_at: r.get(10)?,
        lang_code: r.get(11)?,
    })
}

/// Directory name for a publication. Only `[A-Za-z0-9_-]` survive, so a
/// hostile symbol cannot escape the publications directory.
fn dir_name(info: &PublicationInfo) -> Result<String> {
    let clean = |s: &str| -> String {
        s.chars()
            .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
            .collect()
    };
    let symbol = clean(&info.symbol);
    if symbol.is_empty() {
        return Err(Error::UnsafePath(info.symbol.clone()));
    }
    let issue = clean(&info.issue_tag);
    Ok(if issue.is_empty() || issue == "0" {
        format!("{symbol}_{}", info.meps_language)
    } else {
        format!("{symbol}_{}_{issue}", info.meps_language)
    })
}

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

/// Language code from a manifest name like `nwtsty_X.jwpub` or
/// `wp_X_202609.jwpub` (`<symbol>_<code>[_<issue>]`, see docs/FORMAT.md).
pub fn lang_code_from_name(name: &str) -> Option<String> {
    let stem = name.strip_suffix(".jwpub").unwrap_or(name);
    let code = stem.split('_').nth(1)?;
    (!code.is_empty()
        && code.len() <= 8
        && code
            .chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '-'))
    .then(|| code.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_dir_moves_the_old_folder_once() {
        let tmp = tempfile::tempdir().unwrap();
        fs::create_dir(tmp.path().join("jwlinux")).unwrap();
        fs::write(tmp.path().join("jwlinux").join("index.sqlite"), "x").unwrap();
        let dir = app_dir(tmp.path());
        assert_eq!(dir, tmp.path().join("pergament"));
        assert!(dir.join("index.sqlite").is_file());
        assert!(!tmp.path().join("jwlinux").exists());
        // An existing new folder is never replaced.
        fs::create_dir(tmp.path().join("jwlinux")).unwrap();
        app_dir(tmp.path());
        assert!(tmp.path().join("jwlinux").exists());
    }

    #[test]
    fn lang_codes_from_names() {
        assert_eq!(lang_code_from_name("nwtsty_X.jwpub").as_deref(), Some("X"));
        assert_eq!(
            lang_code_from_name("wp_X_202609.jwpub").as_deref(),
            Some("X")
        );
        assert_eq!(lang_code_from_name("fg_E.jwpub").as_deref(), Some("E"));
        assert_eq!(
            lang_code_from_name("w_ASL_202601.jwpub").as_deref(),
            Some("ASL")
        );
        for bad in [
            "nwtsty.jwpub",
            "a_x.jwpub",
            "a_/etc.jwpub",
            "a__b.jwpub",
            "",
        ] {
            assert_eq!(lang_code_from_name(bad), None, "{bad}");
        }
    }

    #[test]
    fn migrates_old_index() {
        let tmp = tempfile::tempdir().unwrap();
        let conn = Connection::open(tmp.path().join("index.sqlite")).unwrap();
        conn.execute_batch(
            "CREATE TABLE publication (id INTEGER PRIMARY KEY, symbol TEXT NOT NULL,
                 meps_language INTEGER NOT NULL, issue_tag TEXT NOT NULL, year INTEGER NOT NULL,
                 title TEXT NOT NULL, short_title TEXT, publication_type TEXT,
                 dir_name TEXT NOT NULL UNIQUE, db_file TEXT NOT NULL, contents_hash TEXT NOT NULL,
                 imported_at INTEGER NOT NULL, UNIQUE (symbol, meps_language, issue_tag));
             INSERT INTO publication VALUES (1, 'old', 0, '0', 2020, 'Old', NULL, NULL,
                 'old_0', 'old.db', 'aa', 0);",
        )
        .unwrap();
        drop(conn);
        let lib = Library::open(tmp.path()).unwrap();
        let all = lib.list().unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].lang_code, None);
        // Opening again must not fail on the existing column.
        drop(lib);
        Library::open(tmp.path()).unwrap();
    }

    fn media_entry(label: &str) -> MediaEntry {
        MediaEntry {
            id: 0,
            key: "pub-nwtsv_X_1_VIDEO".into(),
            lang_code: "X".into(),
            title: "Einführung".into(),
            kind: MediaKind::Video,
            label: label.into(),
            file_name: format!("{}.mp4", media_stem("pub-nwtsv_X_1_VIDEO", "X", label)),
            subtitle_file: Some(format!(
                "{}.vtt",
                media_stem("pub-nwtsv_X_1_VIDEO", "X", label)
            )),
            size: 4,
            duration: Some(327.7),
            image: None,
            downloaded_at: 1,
        }
    }

    fn write_media(lib: &Library, e: &MediaEntry) {
        fs::create_dir_all(lib.media_dir()).unwrap();
        fs::write(lib.media_dir().join(&e.file_name), "data").unwrap();
        fs::write(
            lib.media_dir().join(e.subtitle_file.as_ref().unwrap()),
            "WEBVTT",
        )
        .unwrap();
    }

    #[test]
    fn media_downloads() {
        let tmp = tempfile::tempdir().unwrap();
        let lib = Library::open(tmp.path()).unwrap();
        assert!(lib.list_media().unwrap().is_empty());

        let a = media_entry("240p");
        write_media(&lib, &a);
        let saved = lib.add_media(&a).unwrap();
        assert_eq!(saved.label, "240p");
        assert_eq!(lib.list_media().unwrap().len(), 1);

        // Another quality of the same recording replaces the first one.
        let b = media_entry("480p");
        write_media(&lib, &b);
        let saved = lib.add_media(&b).unwrap();
        assert_eq!(lib.list_media().unwrap(), vec![saved.clone()]);
        assert!(!lib.media_dir().join(&a.file_name).exists());
        assert!(!lib.media_dir().join(a.subtitle_file.unwrap()).exists());
        assert!(lib.media_dir().join(&b.file_name).is_file());

        // A file that went missing hides the entry.
        fs::remove_file(lib.media_dir().join(&b.file_name)).unwrap();
        assert!(lib.list_media().unwrap().is_empty());
        write_media(&lib, &b);

        assert!(lib.remove_media(saved.id).unwrap());
        assert!(!lib.remove_media(saved.id).unwrap());
        assert!(!lib.media_dir().join(&b.file_name).exists());
        assert!(lib.list_media().unwrap().is_empty());
    }

    #[test]
    fn media_stems_are_plain() {
        assert_eq!(
            media_stem("pub-a_X_1_VIDEO", "X", "480p"),
            "pub-a_X_1_VIDEO_X_480p"
        );
        assert_eq!(media_stem("../x/y", "X", ""), "___x_y_X");
    }
}
