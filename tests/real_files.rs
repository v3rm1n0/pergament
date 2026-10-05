//! Tests against user-supplied publications. Skipped unless
//! `JWL_TEST_JWPUB` (study Bible) / `JWL_TEST_JWPUB_WP` (Watchtower) are set.

use std::path::PathBuf;

use jwlinux::Library;
use jwlinux::crypto::ContentKey;
use jwlinux::jwpub;

fn fixture(var: &str) -> Option<PathBuf> {
    match std::env::var_os(var) {
        Some(p) => Some(PathBuf::from(p)),
        None => {
            eprintln!("skipping: {var} not set");
            None
        }
    }
}

/// Import, then decode every Document and compare with ContentLength.
fn import_and_decode_all(path: PathBuf) -> (jwlinux::Entry, rusqlite::Connection, ContentKey) {
    let tmp = tempfile::tempdir().unwrap();
    let mut lib = Library::open(tmp.path()).unwrap();
    let entry = lib.import(&path).unwrap();
    // Keep the DB outside the temp library so the connection outlives it.
    let db_copy = std::env::temp_dir().join(format!(
        "jwlinux-test-{}-{}",
        std::process::id(),
        entry.db_file
    ));
    std::fs::copy(lib.db_path(&entry), &db_copy).unwrap();
    let conn = jwpub::open_db(&db_copy).unwrap();
    std::fs::remove_file(&db_copy).ok();

    let key = ContentKey::new(
        entry.meps_language,
        &entry.symbol,
        entry.year,
        &entry.issue_tag,
    );
    let mut stmt = conn
        .prepare(
            "SELECT DocumentId, Content, ContentLength FROM Document WHERE Content IS NOT NULL",
        )
        .unwrap();
    let mut n = 0;
    let rows = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, Vec<u8>>(1)?,
                r.get::<_, i64>(2)?,
            ))
        })
        .unwrap();
    for row in rows {
        let (id, blob, len) = row.unwrap();
        let html = key
            .decode(&blob)
            .unwrap_or_else(|e| panic!("doc {id}: {e}"));
        assert_eq!(html.len() as i64, len, "doc {id} length");
        n += 1;
    }
    assert!(n > 0);
    drop(stmt);
    (entry, conn, key)
}

#[test]
fn study_bible() {
    let Some(path) = fixture("JWL_TEST_JWPUB") else {
        return;
    };
    let (entry, conn, key) = import_and_decode_all(path);
    assert_eq!(entry.symbol, "nwtsty");
    assert_eq!(entry.issue_tag, "0");

    let verses: i64 = conn
        .query_row("SELECT count(*) FROM BibleVerse", [], |r| r.get(0))
        .unwrap();
    assert_eq!(verses, 31194);
    // Gen 1:2 is BibleVerseId 1; its decoded span carries the verse id.
    let blob: Vec<u8> = conn
        .query_row(
            "SELECT Content FROM BibleVerse WHERE BibleVerseId = 1",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(key.decode(&blob).unwrap().contains(r#"id="v1-1-2""#));
}

#[test]
fn watchtower() {
    let Some(path) = fixture("JWL_TEST_JWPUB_WP") else {
        return;
    };
    let (entry, _conn, _key) = import_and_decode_all(path);
    assert_eq!(entry.symbol, "wp26");
    assert_eq!(entry.issue_tag, "20260900");
}
