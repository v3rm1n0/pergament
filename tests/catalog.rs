use pergament::catalog::Catalog;
use rusqlite::{Connection, params};

/// A tiny catalog with the tables and columns Pergament reads.
fn build(path: &std::path::Path) {
    let c = Connection::open(path).unwrap();
    c.execute_batch(
        "CREATE TABLE Publication (Id INTEGER PRIMARY KEY, MepsLanguageId INTEGER NOT NULL,
             PublicationTypeId INTEGER, IssueTagNumber INTEGER NOT NULL, Title TEXT NOT NULL,
             IssueTitle TEXT, ShortTitle TEXT NOT NULL, CoverTitle TEXT, Year INTEGER NOT NULL,
             Symbol TEXT NOT NULL, KeySymbol TEXT);
         CREATE TABLE PublicationAsset (Id INTEGER PRIMARY KEY, PublicationId INTEGER NOT NULL,
             MepsLanguageId INTEGER NOT NULL, Signature TEXT NOT NULL, Size INTEGER NOT NULL,
             MimeType TEXT, CatalogedOn TEXT);
         CREATE TABLE ImageAsset (Id INTEGER PRIMARY KEY, NameFragment TEXT NOT NULL,
             Width INTEGER NOT NULL DEFAULT 270, Height INTEGER NOT NULL DEFAULT 270);
         CREATE TABLE DatedText (Class INTEGER, Start TEXT, End TEXT, PublicationId INTEGER);
         CREATE TABLE CuratedAsset (ListType INTEGER, SortOrder INTEGER, PublicationAssetId INTEGER);
         CREATE TABLE PublicationAttribute (Name TEXT, Id INTEGER PRIMARY KEY);
         CREATE TABLE PublicationAttributeMap (PublicationId INTEGER, PublicationAttributeId INTEGER);
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
            "INSERT INTO PublicationAsset VALUES (?1, ?1, ?2, ?3, ?4, 'application/x-jwpub', ?5)",
            params![
                id,
                lang,
                format!("{id:040}"),
                1000 * id,
                format!("2026-10-0{id}T00:00:00+00:00")
            ],
        )
        .unwrap();
    }
    c.execute_batch(
        "INSERT INTO DatedText VALUES (106, '2026-09-07', '2026-11-01', 1);
         INSERT INTO CuratedAsset VALUES (2, 1, 3), (2, 0, 1);
         INSERT INTO PublicationAttribute VALUES ('Convention', 3);
         INSERT INTO PublicationAttributeMap VALUES (2, 3);",
    )
    .unwrap();
    // Language evidence: two German covers, one stray English one, one universal.
    let images = [
        (1, 1, "images/aa/2026000_X_cvr.jpg"),
        (2, 2, "images/bb/2025000_X_lsr-240x120.jpg"),
        (3, 3, "images/cc/1001_E_cvr.jpg"),
        (4, 4, "images/dd/2026000_E_cvr.jpg"),
        (5, 1, "images/ee/2026000_univ_sqr-120.jpg"),
    ];
    for (img, asset, name) in images {
        c.execute(
            "INSERT INTO ImageAsset (Id, NameFragment) VALUES (?1, ?2)",
            params![img, name],
        )
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

#[test]
fn categories_curated_new_and_dated() {
    use pergament::catalog::{CONVENTION, DATED_MEETING_WORKBOOK, LIST_TEACHING_TOOLBOX};
    let (_t, c) = open();
    assert_eq!(c.categories(2).unwrap(), vec![(14, 3), (CONVENTION, 1)]);
    let convention = c.by_category(2, CONVENTION, 10).unwrap();
    assert_eq!(convention[0].symbol, "wp25");
    assert_eq!(convention[0].attributes, ["Convention"]);
    let toolbox: Vec<_> = c
        .curated(2, LIST_TEACHING_TOOLBOX)
        .unwrap()
        .into_iter()
        .map(|i| i.symbol)
        .collect();
    assert_eq!(toolbox, ["wp26", "nwtsty"]);
    assert_eq!(c.whats_new(2, 1).unwrap()[0].symbol, "nwtsty");
    let week = c.dated(2, DATED_MEETING_WORKBOOK, "2026-10-07").unwrap();
    assert_eq!(week.len(), 1);
    assert_eq!(week[0].0.symbol, "wp26");
    assert_eq!(week[0].1, "2026-09-07");
    assert!(
        c.dated(2, DATED_MEETING_WORKBOOK, "2027-01-01")
            .unwrap()
            .is_empty()
    );
    // Images: the square one is preferred.
    assert_eq!(
        c.find("wp", 2, Some(20260900))
            .unwrap()
            .unwrap()
            .image
            .as_deref(),
        Some("images/ee/2026000_univ_sqr-120.jpg")
    );
}

#[test]
fn image_paths() {
    use pergament::catalog::safe_image_path;
    assert!(safe_image_path("images/33/1102021352_univ_sqr-120x120.jpg"));
    for bad in [
        "images/../x.jpg",
        "/images/a/b.jpg",
        "images/a/../../x",
        "x/a/b.jpg",
        "images/a/b/c.jpg",
        "images/a/.x",
    ] {
        assert!(!safe_image_path(bad), "{bad}");
    }
}

/// Against the real catalog (values checked against the original app on
/// 2026-10-05). Set PERGAMENT_TEST_CATALOG to a catalog.db.
#[test]
fn real_catalog() {
    use pergament::catalog::*;
    let Some(path) = std::env::var_os("PERGAMENT_TEST_CATALOG") else {
        eprintln!("skipping: PERGAMENT_TEST_CATALOG not set");
        return;
    };
    let c = Catalog::open(std::path::Path::new(&path), "test".into()).unwrap();
    let meetings = c.dated(2, DATED_MEETING_WORKBOOK, "2026-10-07").unwrap();
    assert_eq!(
        (meetings[0].0.key_symbol.as_str(), meetings[0].0.issue_tag),
        ("mwb", 20260900)
    );
    let study = c.dated(2, DATED_WATCHTOWER_STUDY, "2026-10-07").unwrap();
    assert_eq!(
        (study[0].0.key_symbol.as_str(), study[0].0.issue_tag),
        ("w", 20260800)
    );
    assert_eq!(
        (study[0].1.as_str(), study[0].2.as_str()),
        ("2026-10-05", "2026-11-01")
    );
    let toolbox = c.curated(2, LIST_TEACHING_TOOLBOX).unwrap();
    assert!(toolbox.iter().any(|i| i.key_symbol == "lff"));
    assert_eq!(c.whats_new(2, 1).unwrap()[0].key_symbol, "sjj");
    let cats = c.categories(2).unwrap();
    assert!(cats.contains(&(14, 1745)));
    assert!(
        c.curated(2, LIST_MEETINGS)
            .unwrap()
            .iter()
            .any(|i| i.key_symbol == "S-38")
    );
    assert!(toolbox.iter().all(|i| i.image.is_some()));
    let other = c.curated(2, LIST_MEETINGS).unwrap();
    // S-38 has no image in the catalog.
    assert!(
        other
            .iter()
            .filter(|i| i.key_symbol != "S-38")
            .all(|i| i.image.is_some())
    );
    assert!(meetings[0].0.image.is_some(), "{:?}", meetings[0].0);
}
