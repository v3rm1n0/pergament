//! The local library: imported publications plus a SQLite index.
//!
//! Layout below the root (default `$XDG_DATA_HOME/jwlinux`):
//! `index.sqlite` and `publications/<symbol>_<lang>[_<issue>]/` holding the
//! unpacked `contents` of each publication.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{Connection, OptionalExtension, params};

use crate::jwpub::{self, JwPub, Limits, PublicationInfo};
use crate::{Error, Result};

const PUBLICATIONS: &str = "publications";

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

impl Library {
    /// Open the library at `$XDG_DATA_HOME/jwlinux`.
    pub fn open_default() -> Result<Self> {
        Self::open(dirs::data_dir().ok_or(Error::NoDataDir)?.join("jwlinux"))
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
}
