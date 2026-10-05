//! The public publication catalog (`catalogs/publications/v4`), cached
//! locally, plus the derived MEPS-id ↔ language-code table. See
//! docs/FORMAT.md ("Download services").

use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{self, BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use flate2::read::GzDecoder;
use rusqlite::{Connection, OptionalExtension, params};
use serde::Deserialize;

use crate::jwpub;
use crate::net::{Client, Expected};
use crate::{Error, Result};

pub const MANIFEST_URL: &str = "https://app.jw-cdn.org/catalogs/publications/v4/manifest.json";

/// Upper bound for the unpacked catalog (216 MB in October 2026).
const MAX_CATALOG_SIZE: u64 = 4 << 30;

#[derive(Debug, Deserialize)]
struct CatalogManifest {
    current: String,
}

/// One downloadable publication.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogItem {
    pub key_symbol: String,
    pub symbol: String,
    pub meps_language: i64,
    pub issue_tag: i64,
    pub year: i64,
    pub title: String,
    pub issue_title: Option<String>,
    /// Size of the `.jwpub` file.
    pub size: u64,
    /// SHA-1 of the `.jwpub` file.
    pub sha1: String,
}

#[derive(Debug)]
pub struct Catalog {
    conn: Connection,
    version: String,
    languages: HashMap<i64, String>,
}

impl Catalog {
    /// Open the newest cached catalog, downloading one if there is none, or if
    /// `refresh` is set (or the cache is older than `max_age`) and the server
    /// has a newer version.
    pub fn load(
        client: &Client,
        cache_dir: &Path,
        refresh: bool,
        max_age: Duration,
        progress: &mut dyn FnMut(u64, Option<u64>),
    ) -> Result<Self> {
        fs::create_dir_all(cache_dir)?;
        let cached = newest_cached(cache_dir)?;
        let stale = cached.as_ref().is_none_or(|(_, path)| {
            refresh
                || fs::metadata(path)
                    .and_then(|m| m.modified())
                    .ok()
                    .and_then(|t| SystemTime::now().duration_since(t).ok())
                    .is_none_or(|age| age > max_age)
        });
        if !stale {
            let (version, path) = cached.expect("checked above");
            return Self::open(&path, version);
        }

        let manifest: CatalogManifest = client.get_json(MANIFEST_URL)?;
        let version = manifest.current;
        if version.is_empty() || !version.chars().all(|c| c.is_ascii_hexdigit() || c == '-') {
            return Err(Error::Http(format!(
                "unexpected catalog version {version:?}"
            )));
        }
        let db_path = cache_dir.join(format!("catalog-{version}.db"));
        if !db_path.exists() {
            let gz = cache_dir.join(format!("catalog-{version}.db.gz"));
            let url =
                format!("https://app.jw-cdn.org/catalogs/publications/v4/{version}/catalog.db.gz");
            client.download(&url, &gz, &Expected::default(), progress)?;
            let tmp = cache_dir.join(format!(".catalog-{version}.db.tmp"));
            gunzip_bounded(&gz, &tmp, MAX_CATALOG_SIZE)?;
            fs::rename(&tmp, &db_path)?;
            fs::remove_file(&gz)?;
        } else {
            // Mark as checked so the next run does not ask again right away.
            File::options()
                .append(true)
                .open(&db_path)?
                .set_modified(SystemTime::now())?;
        }
        // Drop older catalogs.
        for entry in fs::read_dir(cache_dir)? {
            let p = entry?.path();
            let name = p.file_name().and_then(|n| n.to_str()).unwrap_or_default();
            if name.starts_with("catalog-") && p != db_path {
                let _ = fs::remove_file(&p);
            }
        }
        Self::open(&db_path, version)
    }

    /// Open the newest cached catalog without any network access.
    pub fn open_cached(cache_dir: &Path) -> Result<Option<Self>> {
        if !cache_dir.exists() {
            return Ok(None);
        }
        newest_cached(cache_dir)?
            .map(|(version, path)| Self::open(&path, version))
            .transpose()
    }

    /// Open a catalog database file directly.
    pub fn open(path: &Path, version: String) -> Result<Self> {
        let conn = jwpub::open_db(path)?;
        let ok: Option<i64> = conn
            .query_row(
                "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'Publication'",
                [],
                |r| r.get(0),
            )
            .optional()?;
        if ok.is_none() {
            return Err(Error::InvalidDatabase {
                path: path.to_owned(),
                reason: "not a publication catalog".into(),
            });
        }
        let languages = derive_languages(&conn)?;
        Ok(Self {
            conn,
            version,
            languages,
        })
    }

    pub fn version(&self) -> &str {
        &self.version
    }

    /// Derived MEPS id → language code (e.g. 2 → "X").
    pub fn language_code(&self, meps_language: i64) -> Option<&str> {
        self.languages.get(&meps_language).map(String::as_str)
    }

    /// Derived language code → MEPS id.
    pub fn meps_language(&self, code: &str) -> Option<i64> {
        self.languages
            .iter()
            .find(|(_, c)| c.eq_ignore_ascii_case(code))
            .map(|(id, _)| *id)
    }

    /// Publications in a language whose title or symbol contains `query`
    /// (all of them if `query` is empty), newest first.
    pub fn search(
        &self,
        meps_language: i64,
        query: &str,
        limit: usize,
    ) -> Result<Vec<CatalogItem>> {
        let pattern = format!(
            "%{}%",
            query
                .replace('\\', "\\\\")
                .replace('%', "\\%")
                .replace('_', "\\_")
        );
        let mut stmt = self.conn.prepare(&format!(
            "{ITEM_SELECT}
             WHERE p.MepsLanguageId = ?1
               AND (p.Title LIKE ?2 ESCAPE '\\' OR p.ShortTitle LIKE ?2 ESCAPE '\\'
                    OR IFNULL(p.IssueTitle, '') LIKE ?2 ESCAPE '\\'
                    OR IFNULL(p.CoverTitle, '') LIKE ?2 ESCAPE '\\'
                    OR p.Symbol LIKE ?2 ESCAPE '\\' OR p.KeySymbol LIKE ?2 ESCAPE '\\')
             ORDER BY p.Year DESC, p.IssueTagNumber DESC, p.KeySymbol
             LIMIT ?3"
        ))?;
        let rows = stmt.query_map(params![meps_language, pattern, limit as i64], item_from_row)?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Find one publication by key symbol (`wp`) or dated symbol (`wp26`).
    /// Without an issue the newest issue is returned.
    pub fn find(
        &self,
        symbol: &str,
        meps_language: i64,
        issue_tag: Option<i64>,
    ) -> Result<Option<CatalogItem>> {
        Ok(self
            .conn
            .query_row(
                &format!(
                    "{ITEM_SELECT}
                     WHERE p.MepsLanguageId = ?1 AND (p.KeySymbol = ?2 OR p.Symbol = ?2)
                       AND (?3 IS NULL OR p.IssueTagNumber = ?3)
                     ORDER BY p.IssueTagNumber DESC LIMIT 1"
                ),
                params![meps_language, symbol, issue_tag],
                item_from_row,
            )
            .optional()?)
    }
}

const ITEM_SELECT: &str =
    "SELECT p.KeySymbol, p.Symbol, p.MepsLanguageId, p.IssueTagNumber, p.Year,
        p.Title, p.IssueTitle, a.Size, a.Signature
     FROM Publication p JOIN PublicationAsset a ON a.PublicationId = p.Id";

fn item_from_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<CatalogItem> {
    Ok(CatalogItem {
        key_symbol: r.get(0)?,
        symbol: r.get(1)?,
        meps_language: r.get(2)?,
        issue_tag: r.get(3)?,
        year: r.get(4)?,
        title: r.get(5)?,
        issue_title: r.get(6)?,
        size: r.get::<_, i64>(7)?.max(0) as u64,
        sha1: r.get(8)?,
    })
}

fn newest_cached(dir: &Path) -> Result<Option<(String, PathBuf)>> {
    let mut best: Option<(SystemTime, String, PathBuf)> = None;
    for entry in fs::read_dir(dir)? {
        let p = entry?.path();
        let Some(version) = p
            .file_name()
            .and_then(|n| n.to_str())
            .and_then(|n| n.strip_prefix("catalog-"))
            .and_then(|n| n.strip_suffix(".db"))
            .map(str::to_owned)
        else {
            continue;
        };
        let modified = fs::metadata(&p)?.modified()?;
        if best.as_ref().is_none_or(|(t, _, _)| modified > *t) {
            best = Some((modified, version, p));
        }
    }
    Ok(best.map(|(_, v, p)| (v, p)))
}

fn gunzip_bounded(src: &Path, dest: &Path, limit: u64) -> Result<()> {
    let mut out = BufWriter::new(File::create(dest)?);
    let n = io::copy(
        &mut GzDecoder::new(File::open(src)?).take(limit + 1),
        &mut out,
    )?;
    out.flush()?;
    if n > limit {
        drop(out);
        let _ = fs::remove_file(dest);
        return Err(Error::TooLarge("catalog".into()));
    }
    Ok(())
}

/// Language code from an image name like `images/ab/2026000_X_cvr.jpg`.
pub fn code_from_image_name(name: &str) -> Option<&str> {
    const KINDS: &[&str] = &["cvr", "lsr", "sqr", "pnr", "lss", "sqs", "cnt", "wss"];
    let file = name.rsplit('/').next()?;
    let parts: Vec<&str> = file.split('_').collect();
    parts.windows(2).skip(1).find_map(|w| {
        let (code, next) = (w[0], w[1]);
        let is_code = (1..=4).contains(&code.len()) && code.chars().all(|c| c.is_ascii_uppercase());
        (is_code && KINDS.iter().any(|k| next.starts_with(k))).then_some(code)
    })
}

/// Majority vote of language codes in image names per MEPS language id.
fn derive_languages(conn: &Connection) -> Result<HashMap<i64, String>> {
    let mut stmt = conn.prepare(
        "SELECT a.MepsLanguageId, i.NameFragment FROM ImageAsset i
         JOIN PublicationAssetImageMap m ON m.ImageAssetId = i.Id
         JOIN PublicationAsset a ON a.Id = m.PublicationAssetId
         WHERE i.NameFragment NOT LIKE '%univ%'",
    )?;
    let mut votes: HashMap<i64, HashMap<String, u32>> = HashMap::new();
    let rows = stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))?;
    for row in rows {
        let (lang, name) = row?;
        if let Some(code) = code_from_image_name(&name) {
            *votes
                .entry(lang)
                .or_default()
                .entry(code.to_owned())
                .or_default() += 1;
        }
    }
    Ok(votes
        .into_iter()
        .filter_map(|(lang, v)| {
            v.into_iter()
                .max_by(|a, b| a.1.cmp(&b.1).then_with(|| b.0.cmp(&a.0)))
                .map(|(code, _)| (lang, code))
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn image_codes() {
        assert_eq!(
            code_from_image_name("images/ab/2026000_X_cvr.jpg"),
            Some("X")
        );
        assert_eq!(code_from_image_name("1102012181_E_cvr.jpg"), Some("E"));
        assert_eq!(
            code_from_image_name("images/3d/1102_ASL_lsr-1200x600.jpg"),
            Some("ASL")
        );
        assert_eq!(
            code_from_image_name("images/2b/302014021_univ_sqr-126.jpg"),
            None
        );
        assert_eq!(code_from_image_name("X_cvr.jpg"), None);
        assert_eq!(code_from_image_name("a_TOOLONG_cvr.jpg"), None);
    }
}
