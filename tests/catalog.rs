use jwlinux::catalog::Catalog;
use rusqlite::{Connection, params};

/// A tiny catalog with the tables and columns jwlinux reads.
fn build(path: &std::path::Path) {
    let c = Connection::open(path).unwrap();
    c.execute_batch(
        "CREATE TABLE Publication (Id INTEGER PRIMARY KEY, MepsLanguageId INTEGER NOT NULL,
             PublicationTypeId INTEGER, IssueTagNumber INTEGER NOT NULL, Title TEXT NOT NULL,
             IssueTitle TEXT, ShortTitle TEXT NOT NULL, CoverTitle TEXT, Year INTEGER NOT NULL,
             Symbol TEXT NOT NULL, KeySymbol TEXT);
         CREATE TABLE PublicationAsset (Id INTEGER PRIMARY KEY, PublicationId INTEGER NOT NULL,
             MepsLanguageId INTEGER NOT NULL, Signature TEXT NOT NULL, Size INTEGER NOT NULL,
             MimeType TEXT);
         CREATE TABLE ImageAsset (Id INTEGER PRIMARY KEY, NameFragment TEXT NOT NULL);
         CREATE TABLE PublicationAssetImageMap (PublicationAssetId INTEGER, ImageAssetId INTEGER);",
    )
    .unwrap();
    let pubs = [
        (
            1,
            2,
            20260900,
            "Der Wachtturm 2026",
            "Nr. 1 2026",
            2026,
            "wp26",
            "wp",
        ),
        (
            2,
            2,
            20250900,
            "Der Wachtturm 2025",
            "Nr. 1 2025",
            2025,
            "wp25",
            "wp",
        ),
        (
            3,
            2,
            0,
            "Studienbibel 100%_echt",
            "",
            2025,
            "nwtsty",
            "nwtsty",
        ),
        (
            4,
            0,
            20260900,
            "The Watchtower 2026",
            "No. 1 2026",
            2026,
            "wp26",
            "wp",
        ),
    ];
    for (id, lang, issue, title, issue_title, year, sym, key) in pubs {
        c.execute(
            "INSERT INTO Publication VALUES (?1, ?2, 14, ?3, ?4, ?5, ?4, NULL, ?6, ?7, ?8)",
            params![id, lang, issue, title, issue_title, year, sym, key],
        )
        .unwrap();
        c.execute(
            "INSERT INTO PublicationAsset VALUES (?1, ?1, ?2, ?3, ?4, 'application/x-jwpub')",
            params![id, lang, format!("{id:040}"), 1000 * id],
        )
        .unwrap();
    }
    // Language evidence: two German covers, one stray English one, one universal.
    let images = [
        (1, 1, "images/aa/2026000_X_cvr.jpg"),
        (2, 2, "images/bb/2025000_X_lsr-240x120.jpg"),
        (3, 3, "images/cc/1001_E_cvr.jpg"),
        (4, 4, "images/dd/2026000_E_cvr.jpg"),
        (5, 1, "images/ee/2026000_univ_sqr-120.jpg"),
    ];
    for (img, asset, name) in images {
        c.execute("INSERT INTO ImageAsset VALUES (?1, ?2)", params![img, name])
            .unwrap();
        c.execute(
            "INSERT INTO PublicationAssetImageMap VALUES (?1, ?2)",
            params![asset, img],
        )
        .unwrap();
    }
}

fn open() -> (tempfile::TempDir, Catalog) {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("catalog-0000.db");
    build(&path);
    let cat = Catalog::open(&path, "0000".into()).unwrap();
    (tmp, cat)
}

#[test]
fn derives_languages_by_majority() {
    let (_t, c) = open();
    assert_eq!(c.language_code(2), Some("X"));
    assert_eq!(c.language_code(0), Some("E"));
    assert_eq!(c.meps_language("x"), Some(2));
    assert_eq!(c.meps_language("ZZ"), None);
}

#[test]
fn search_by_title_newest_first() {
    let (_t, c) = open();
    let r = c.search(2, "wachtturm", 10).unwrap();
    let issues: Vec<_> = r.iter().map(|i| i.issue_tag).collect();
    assert_eq!(issues, vec![20260900, 20250900]);
    assert_eq!(r[0].sha1, format!("{:040}", 1));
    assert_eq!(r[0].size, 1000);
    assert_eq!(c.search(2, "", 10).unwrap().len(), 3);
    assert_eq!(c.search(0, "wachtturm", 10).unwrap().len(), 0);
}

#[test]
fn search_escapes_like_wildcards() {
    let (_t, c) = open();
    assert_eq!(c.search(2, "100%_", 10).unwrap().len(), 1);
    assert_eq!(c.search(2, "%", 10).unwrap().len(), 1);
    assert_eq!(c.search(2, "_", 10).unwrap().len(), 1);
}

#[test]
fn find_by_key_or_dated_symbol() {
    let (_t, c) = open();
    assert_eq!(c.find("wp", 2, None).unwrap().unwrap().issue_tag, 20260900);
    assert_eq!(
        c.find("wp", 2, Some(20250900)).unwrap().unwrap().symbol,
        "wp25"
    );
    assert_eq!(
        c.find("wp25", 2, None).unwrap().unwrap().issue_tag,
        20250900
    );
    assert!(c.find("nwtsty", 0, None).unwrap().is_none());
}

#[test]
fn open_cached_picks_existing_catalog() {
    let tmp = tempfile::tempdir().unwrap();
    assert!(Catalog::open_cached(tmp.path()).unwrap().is_none());
    build(&tmp.path().join("catalog-abcd.db"));
    let c = Catalog::open_cached(tmp.path()).unwrap().unwrap();
    assert_eq!(c.version(), "abcd");
}

#[test]
fn rejects_non_catalog_database() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("x.db");
    Connection::open(&path)
        .unwrap()
        .execute_batch("CREATE TABLE Other (x)")
        .unwrap();
    assert!(Catalog::open(&path, "x".into()).is_err());
}

#[test]
fn search_matches_every_word_anywhere() {
    let (_t, c) = open();
    // Words appear in different places and order: title vs issue title.
    assert_eq!(c.search(2, "Wachtturm Nr. 2026", 10).unwrap().len(), 1);
    assert_eq!(c.search(2, "2025 wachtturm", 10).unwrap().len(), 1);
    assert_eq!(c.search(2, "wachtturm studienbibel", 10).unwrap().len(), 0);
    assert_eq!(c.search(2, "   ", 10).unwrap().len(), 3);
}
