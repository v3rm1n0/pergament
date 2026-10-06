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
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
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
    /// `Publication.PublicationTypeId`, see [`category_name`].
    pub publication_type: i64,
    pub short_title: Option<String>,
    /// When it was added to the catalog (RFC 3339).
    pub cataloged_on: Option<String>,
    /// Square cover image as a path below [`IMAGE_BASE`].
    pub image: Option<String>,
    /// `PublicationAttribute` names such as `Archive` or `Study`.
    pub attributes: Vec<String>,
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

    /// Publications in a language where every word of `query` appears in a
    /// title or symbol (all of them if `query` is empty), newest first.
    pub fn search(
        &self,
        meps_language: i64,
        query: &str,
        limit: usize,
    ) -> Result<Vec<CatalogItem>> {
        let words: Vec<String> = query
            .split_whitespace()
            .take(16)
            .map(|w| {
                format!(
                    "%{}%",
                    w.replace('\\', "\\\\")
                        .replace('%', "\\%")
                        .replace('_', "\\_")
                )
            })
            .collect();
        // One condition per word; parameters ?3.. hold the patterns.
        let conditions: String = (0..words.len())
            .map(|i| {
                let p = format!("?{}", i + 3);
                format!(
                    " AND (p.Title LIKE {p} ESCAPE '\\' OR p.ShortTitle LIKE {p} ESCAPE '\\'
                      OR IFNULL(p.IssueTitle, '') LIKE {p} ESCAPE '\\'
                      OR IFNULL(p.CoverTitle, '') LIKE {p} ESCAPE '\\'
                      OR p.Symbol LIKE {p} ESCAPE '\\' OR p.KeySymbol LIKE {p} ESCAPE '\\')"
                )
            })
            .collect();
        let mut stmt = self.conn.prepare(&format!(
            "{ITEM_SELECT}
             WHERE p.MepsLanguageId = ?1{conditions}
             ORDER BY p.Year DESC, p.IssueTagNumber DESC, p.KeySymbol
             LIMIT ?2"
        ))?;
        let mut values: Vec<rusqlite::types::Value> =
            vec![meps_language.into(), (limit as i64).into()];
        values.extend(words.into_iter().map(Into::into));
        let rows = stmt.query_map(rusqlite::params_from_iter(values), item_from_row)?;
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

impl Catalog {
    fn items(&self, sql_tail: &str, params: impl rusqlite::Params) -> Result<Vec<CatalogItem>> {
        let mut stmt = self.conn.prepare(&format!("{ITEM_SELECT} {sql_tail}"))?;
        let rows = stmt.query_map(params, item_from_row)?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Categories with publications in a language: `(type id, count)`, in the
    /// order of [`CATEGORIES`]. Convention releases are an attribute and are
    /// reported as [`CONVENTION`].
    pub fn categories(&self, meps_language: i64) -> Result<Vec<(i64, i64)>> {
        let mut stmt = self.conn.prepare(
            "SELECT PublicationTypeId, count(*) FROM Publication
             WHERE MepsLanguageId = ?1 GROUP BY PublicationTypeId",
        )?;
        let mut counts: HashMap<i64, i64> = stmt
            .query_map([meps_language], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<rusqlite::Result<_>>()?;
        if let Some(n) = counts.remove(&PROGRAMS_OLD) {
            *counts.entry(PROGRAMS).or_default() += n;
        }
        let convention: i64 = self.conn.query_row(
            "SELECT count(*) FROM PublicationAttributeMap m
             JOIN PublicationAttribute a ON a.Id = m.PublicationAttributeId
             JOIN Publication p ON p.Id = m.PublicationId
             WHERE a.Name = 'Convention' AND p.MepsLanguageId = ?1",
            [meps_language],
            |r| r.get(0),
        )?;
        Ok(CATEGORIES
            .iter()
            .filter_map(|(id, _)| match *id {
                CONVENTION => (convention > 0).then_some((CONVENTION, convention)),
                id => counts.get(&id).map(|n| (id, *n)),
            })
            .collect())
    }

    /// Publications of a category, newest first.
    pub fn by_category(
        &self,
        meps_language: i64,
        category: i64,
        limit: usize,
    ) -> Result<Vec<CatalogItem>> {
        if category == CONVENTION {
            return self.items(
                "WHERE p.MepsLanguageId = ?1 AND p.Id IN (
                     SELECT m.PublicationId FROM PublicationAttributeMap m
                     JOIN PublicationAttribute t ON t.Id = m.PublicationAttributeId
                     WHERE t.Name = 'Convention')
                 ORDER BY p.Year DESC, p.IssueTagNumber DESC, p.Title LIMIT ?2",
                params![meps_language, limit as i64],
            );
        }
        // Older circuit assembly programs have their own type id.
        let second = if category == PROGRAMS {
            PROGRAMS_OLD
        } else {
            category
        };
        self.items(
            "WHERE p.MepsLanguageId = ?1 AND p.PublicationTypeId IN (?2, ?3)
             ORDER BY p.Year DESC, p.IssueTagNumber DESC, p.Title LIMIT ?4",
            params![meps_language, category, second, limit as i64],
        )
    }

    /// A curated list (`CuratedAsset.ListType`, e.g. [`LIST_TEACHING_TOOLBOX`]).
    pub fn curated(&self, meps_language: i64, list: i64) -> Result<Vec<CatalogItem>> {
        self.items(
            "JOIN CuratedAsset c ON c.PublicationAssetId = a.Id
             WHERE p.MepsLanguageId = ?1 AND c.ListType = ?2
             ORDER BY c.SortOrder",
            params![meps_language, list],
        )
    }

    /// Most recently cataloged publications.
    pub fn whats_new(&self, meps_language: i64, limit: usize) -> Result<Vec<CatalogItem>> {
        self.items(
            "WHERE p.MepsLanguageId = ?1 ORDER BY a.CatalogedOn DESC LIMIT ?2",
            params![meps_language, limit as i64],
        )
    }

    /// The newest publication containing the document with this MEPS id.
    pub fn by_document(
        &self,
        meps_language: i64,
        meps_document_id: i64,
    ) -> Result<Option<CatalogItem>> {
        Ok(self
            .items(
                "JOIN PublicationDocument d ON d.PublicationId = p.Id
                 WHERE p.MepsLanguageId = ?1 AND d.DocumentId = ?2
                 ORDER BY p.Year DESC, p.IssueTagNumber DESC LIMIT 1",
                params![meps_language, meps_document_id],
            )?
            .into_iter()
            .next())
    }

    /// Publications whose dated range of `class` (see [`DATED_MEETING_WORKBOOK`]
    /// etc.) contains `date` (`YYYY-MM-DD`): `(item, start, end)`.
    pub fn dated(
        &self,
        meps_language: i64,
        class: i64,
        date: &str,
    ) -> Result<Vec<(CatalogItem, String, String)>> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT * FROM ({ITEM_SELECT}
                 WHERE p.MepsLanguageId = ?1) item
             JOIN (SELECT d.Start, d.End, p2.KeySymbol AS ks, p2.IssueTagNumber AS it
                   FROM DatedText d JOIN Publication p2 ON p2.Id = d.PublicationId
                   WHERE p2.MepsLanguageId = ?1 AND d.Class = ?2 AND ?3 BETWEEN d.Start AND d.End)
               ON ks = item.KeySymbol AND it = item.IssueTagNumber
             ORDER BY 1"
        ))?;
        let rows = stmt.query_map(params![meps_language, class, date], |r| {
            Ok((
                item_from_row(r)?,
                r.get::<_, String>(14)?,
                r.get::<_, String>(15)?,
            ))
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }
}

/// Base URL of catalog images; `CatalogItem::image` is relative to it.
pub const IMAGE_BASE: &str = "https://app.jw-cdn.org/catalogs/publications/";

/// `images/ab/name.jpg` with only safe characters.
pub fn safe_image_path(path: &str) -> bool {
    let mut parts = path.split('/');
    matches!(
        (parts.next(), parts.next(), parts.next(), parts.next()),
        (Some("images"), Some(dir), Some(file), None)
            if !dir.is_empty()
                && dir.chars().all(|c| c.is_ascii_alphanumeric())
                && !file.starts_with('.')
                && file.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
    )
}

/// Pseudo category id for convention releases (an attribute, not a type).
pub const CONVENTION: i64 = -1;

const PROGRAMS: i64 = 31;
/// Type id of the 2017 circuit assembly programs in some languages; shown with [`PROGRAMS`].
const PROGRAMS_OLD: i64 = 12;

/// `PublicationTypeId` → category name, in display order. Derived from the
/// symbols in each type (docs/FORMAT.md, "Publication catalog").
pub const CATEGORIES: &[(i64, &str)] = &[
    (2, "Books"),
    (4, "Brochures and Booklets"),
    (10, "Tracts and Invitations"),
    (22, "Article Series"),
    (14, "Watchtower"),
    (13, "Awake!"),
    (30, "Meeting Workbooks"),
    (7, "Kingdom Ministry"),
    (31, "Programs"),
    (6, "Index"),
    (17, "Guidelines"),
    (CONVENTION, "Convention Releases"),
    (1, "Bible"),
];

pub fn category_name(id: i64) -> Option<&'static str> {
    let id = if id == PROGRAMS_OLD { PROGRAMS } else { id };
    CATEGORIES.iter().find(|(i, _)| *i == id).map(|(_, n)| *n)
}

/// `CuratedAsset.ListType` values (docs/FORMAT.md).
pub const LIST_MEETINGS: i64 = 0;
pub const LIST_TEACHING_TOOLBOX: i64 = 2;

/// `DatedText.Class` values (docs/FORMAT.md).
pub const DATED_DAILY_TEXT: i64 = 4;
pub const DATED_WATCHTOWER_STUDY: i64 = 68;
pub const DATED_MEETING_WORKBOOK: i64 = 106;

const ITEM_SELECT: &str =
    "SELECT p.KeySymbol, p.Symbol, p.MepsLanguageId, p.IssueTagNumber, p.Year,
        p.Title, p.IssueTitle, a.Size, a.Signature, p.PublicationTypeId, p.ShortTitle,
        a.CatalogedOn,
        (SELECT i.NameFragment FROM PublicationAssetImageMap m
           JOIN ImageAsset i ON i.Id = m.ImageAssetId
         WHERE m.PublicationAssetId = a.Id
         ORDER BY i.NameFragment LIKE '%\\_sqr%' ESCAPE '\\' DESC, abs(i.Width - 270) LIMIT 1),
        (SELECT group_concat(t.Name, '|') FROM PublicationAttributeMap m2
           JOIN PublicationAttribute t ON t.Id = m2.PublicationAttributeId
         WHERE m2.PublicationId = p.Id)
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
        publication_type: r.get(9)?,
        short_title: r.get(10)?,
        cataloged_on: r.get(11)?,
        image: r
            .get::<_, Option<String>>(12)?
            .filter(|p| safe_image_path(p)),
        attributes: r
            .get::<_, Option<String>>(13)?
            .map(|a| a.split('|').map(str::to_owned).collect())
            .unwrap_or_default(),
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
