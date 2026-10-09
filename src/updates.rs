//! Updated versions of downloaded publications: the catalog lists a file
//! with a different signature than the one that was imported.

use std::fs;
use std::path::Path;

use crate::Result;
use crate::catalog::{Catalog, CatalogItem};
use crate::library::{Entry, Library};

/// A downloaded publication that has a newer version in the catalog.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Update {
    /// Directory name of the installed publication.
    pub dir: String,
    /// Language code to download it in.
    pub lang_code: String,
    /// Unpacked size of what is installed.
    pub installed_size: u64,
    /// The version to download.
    pub item: CatalogItem,
}

/// Total size of the files below `dir`, or `None` if it cannot be read.
fn dir_size(dir: &Path) -> Option<u64> {
    let mut total = 0;
    for entry in fs::read_dir(dir).ok()? {
        let entry = entry.ok()?;
        let meta = entry.metadata().ok()?;
        total += if meta.is_dir() {
            dir_size(&entry.path())?
        } else {
            meta.len()
        };
    }
    Some(total)
}

/// Whether the catalog has a different version of what is installed.
///
/// Publications imported by this version know the SHA-1 of their file, which
/// the catalog lists as `Signature`. Older entries only have their unpacked
/// files, whose total size is the catalog's `ExpandedSize` (checked on 19
/// installed publications: 17 matched exactly, the rest were not listed or
/// were out of date).
pub fn differs(entry: &Entry, installed_size: u64, item: &CatalogItem) -> bool {
    match entry.package_sha1.as_deref() {
        Some(sha1) if !item.sha1.is_empty() => !sha1.eq_ignore_ascii_case(&item.sha1),
        _ => item.expanded_size > 0 && installed_size != item.expanded_size,
    }
}

/// The installed publications that have an update, in library order.
pub fn available(library: &Library, catalog: &Catalog) -> Result<Vec<Update>> {
    let mut found = Vec::new();
    for entry in library.list()? {
        // Undated publications have issue tag 0 in the catalog.
        let issue = entry.issue_tag.parse::<i64>().unwrap_or(0);
        let Some(item) = catalog.find(&entry.symbol, entry.meps_language, Some(issue))? else {
            continue;
        };
        let Some(size) = dir_size(&library.publication_dir(&entry)) else {
            continue;
        };
        let Some(lang_code) = entry.lang_code.clone().or_else(|| {
            catalog
                .language_code(entry.meps_language)
                .map(str::to_owned)
        }) else {
            continue;
        };
        if differs(&entry, size, &item) {
            found.push(Update {
                dir: entry.dir_name,
                lang_code,
                installed_size: size,
                item,
            });
        }
    }
    Ok(found)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(sha1: Option<&str>) -> Entry {
        Entry {
            symbol: "w26".into(),
            meps_language: 2,
            issue_tag: "20260800".into(),
            year: 2026,
            title: "t".into(),
            short_title: None,
            publication_type: None,
            dir_name: "w26_2_20260800".into(),
            db_file: "w.db".into(),
            contents_hash: "h".into(),
            imported_at: 0,
            lang_code: Some("X".into()),
            package_sha1: sha1.map(str::to_owned),
        }
    }

    fn item(sha1: &str, expanded: u64) -> CatalogItem {
        CatalogItem {
            key_symbol: "w".into(),
            symbol: "w26".into(),
            meps_language: 2,
            issue_tag: 20260800,
            year: 2026,
            title: "t".into(),
            issue_title: None,
            size: 1,
            sha1: sha1.into(),
            publication_type: 14,
            short_title: None,
            cataloged_on: None,
            image: None,
            attributes: vec![],
            expanded_size: expanded,
        }
    }

    #[test]
    fn signature_decides_when_known() {
        assert!(!differs(&entry(Some("ABC")), 1, &item("abc", 999)));
        assert!(differs(&entry(Some("abc")), 999, &item("def", 999)));
    }

    #[test]
    fn unpacked_size_decides_otherwise() {
        assert!(!differs(&entry(None), 4_000, &item("def", 4_000)));
        assert!(differs(&entry(None), 3_999, &item("def", 4_000)));
        // Without a size in the catalog there is nothing to compare.
        assert!(!differs(&entry(None), 3_999, &item("def", 0)));
        // A missing signature in the catalog falls back to the size too.
        assert!(differs(&entry(Some("abc")), 1, &item("", 2)));
    }

    #[test]
    fn sizes_add_up_below_subdirectories() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("a"), "123").unwrap();
        fs::create_dir(tmp.path().join("d")).unwrap();
        fs::write(tmp.path().join("d").join("b"), "45").unwrap();
        assert_eq!(dir_size(tmp.path()), Some(5));
        assert_eq!(dir_size(&tmp.path().join("missing")), None);
    }
}
