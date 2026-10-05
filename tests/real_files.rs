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

fn open_library(var: &str) -> Option<(tempfile::TempDir, Library, jwlinux::Entry)> {
    let path = fixture(var)?;
    let tmp = tempfile::tempdir().unwrap();
    let mut lib = Library::open(tmp.path()).unwrap();
    let entry = lib.import(&path).unwrap();
    Some((tmp, lib, entry))
}

/// `verse_ref` must agree with the `id="v{book}-{ch}-{verse}[-part]"` span in
/// every decoded verse.
#[test]
fn study_bible_verse_refs() {
    let Some((_tmp, lib, entry)) = open_library("JWL_TEST_JWPUB") else {
        return;
    };
    let p = jwlinux::Publication::open(&lib, &entry).unwrap();
    let conn = jwpub::open_db(&lib.db_path(&entry)).unwrap();
    let mut stmt = conn
        .prepare("SELECT BibleVerseId, Content FROM BibleVerse")
        .unwrap();
    let rows = stmt
        .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, Vec<u8>>(1)?)))
        .unwrap();
    for row in rows {
        let (id, blob) = row.unwrap();
        let html = p.decode(&blob).unwrap();
        let r = p.verse_ref(id).unwrap();
        let expected = format!("id=\"v{}-{}-{}", r.book, r.chapter, r.verse);
        assert!(
            html.contains(&format!("{expected}\"")) || html.contains(&format!("{expected}-")),
            "verse {id}: {expected} not in {html}"
        );
    }
}

#[test]
fn study_bible_renders_chapters() {
    let Some((_tmp, lib, entry)) = open_library("JWL_TEST_JWPUB") else {
        return;
    };
    let p = jwlinux::Publication::open(&lib, &entry).unwrap();
    let r = jwlinux::Renderer::new(&p, jwlinux::RenderOptions::default());
    // Genesis 1 has footnotes and cross references, Matthew 1 study notes.
    let gen1 = r.chapter(1, 1).unwrap();
    assert!(gen1.contains("class=\"footnotes\""));
    assert!(gen1.contains("class=\"xrefs\""));
    assert!(r.chapter(40, 1).unwrap().contains("class=\"study-notes\""));
    for book in p.bible_books().unwrap() {
        for ch in 1..=book.chapters {
            let html = r.chapter(book.number, ch).unwrap();
            assert!(!html.contains("jwpub:"), "{} {ch}", book.number);
            assert!(!html.contains("data-"), "{} {ch}", book.number);
        }
    }
}

#[test]
fn all_documents_render() {
    for var in ["JWL_TEST_JWPUB", "JWL_TEST_JWPUB_WP"] {
        let Some((_tmp, lib, entry)) = open_library(var) else {
            continue;
        };
        let p = jwlinux::Publication::open(&lib, &entry).unwrap();
        let r = jwlinux::Renderer::new(
            &p,
            jwlinux::RenderOptions {
                media_base: Some("file:///m/".into()),
                standalone: true,
            },
        );
        for d in p.documents().unwrap().into_iter().filter(|d| d.has_content) {
            let html = r.document(d.id).unwrap();
            assert!(!html.contains("jwpub"), "{var} doc {}", d.id);
        }
    }
}

/// Live download from jw.org. Opt-in only: set JWL_TEST_NETWORK=1.
#[test]
fn live_download_matches_fixture() {
    if std::env::var_os("JWL_TEST_NETWORK").is_none() {
        eprintln!("skipping: JWL_TEST_NETWORK not set");
        return;
    }
    let tmp = tempfile::tempdir().unwrap();
    let mut lib = Library::open(tmp.path().join("lib")).unwrap();
    let client = jwlinux::net::Client::new(jwlinux::net::HttpConfig::default());
    let req = jwlinux::remote::Request {
        key_symbol: "wp",
        lang_code: "X",
        issue_tag: Some(20260900),
    };
    let entry = jwlinux::remote::download(
        &client,
        &mut lib,
        &tmp.path().join("dl"),
        &req,
        None,
        &mut |_, _| {},
    )
    .unwrap();
    assert_eq!(entry.symbol, "wp26");
    assert_eq!(entry.issue_tag, "20260900");
    assert_eq!(entry.meps_language, 2);
}

/// Bible links from an article open the imported Bible; publication links
/// resolve by MEPS document id.
#[test]
fn navigation_across_publications() {
    use jwlinux::links::Link;
    use jwlinux::navigate::{self, TargetKind};
    let (Some(bible), Some(wp)) = (fixture("JWL_TEST_JWPUB"), fixture("JWL_TEST_JWPUB_WP")) else {
        return;
    };
    let tmp = tempfile::tempdir().unwrap();
    let mut lib = Library::open(tmp.path()).unwrap();
    let nwtsty = lib.import(&bible).unwrap();
    let wp = lib.import(&wp).unwrap();

    let link = Link::parse("jwlinux://bible/19:23:1-19:23:3").unwrap();
    let t = navigate::resolve(&lib, Some(&wp), &link).unwrap().unwrap();
    assert_eq!(t.publication, nwtsty.dir_name);
    assert_eq!(
        t.kind,
        TargetKind::Chapter {
            book: 19,
            chapter: 23,
            verse: 1
        }
    );
    let page = navigate::page(&lib, &t).unwrap();
    assert!(page.html.contains("v19-23-1"));

    // MEPS document 2026003 is the article "Kann eine bessere Politik …".
    let link = Link::parse("jwlinux://pub/X:2026003/").unwrap();
    let t = navigate::resolve(&lib, Some(&nwtsty), &link)
        .unwrap()
        .unwrap();
    assert_eq!(t.publication, wp.dir_name);
    assert_eq!(t.kind, TargetKind::Document(3));

    let link = Link::parse("jwlinux://pub/X:1/").unwrap();
    assert!(navigate::resolve(&lib, Some(&wp), &link).unwrap().is_none());

    // Verse > 1 scrolls to the verse span.
    let t = navigate::Target {
        publication: nwtsty.dir_name.clone(),
        kind: TargetKind::Chapter {
            book: 19,
            chapter: 23,
            verse: 4,
        },
    };
    assert_eq!(
        navigate::page(&lib, &t).unwrap().fragment.as_deref(),
        Some("v19-23-4-1")
    );
}

#[test]
fn toc_and_covers() {
    let (Some(bible), Some(wp)) = (fixture("JWL_TEST_JWPUB"), fixture("JWL_TEST_JWPUB_WP")) else {
        return;
    };
    let tmp = tempfile::tempdir().unwrap();
    let mut lib = Library::open(tmp.path()).unwrap();
    let nwtsty_entry = lib.import(&bible).unwrap();
    let nwtsty = jwlinux::Publication::open(&lib, &nwtsty_entry).unwrap();
    let toc = nwtsty.toc().unwrap();
    // The study Bible's navigation view: EINFÜHRUNG, BÜCHER, INDEX, ANHANG A-C.
    assert_eq!(toc.len(), 6);
    let books = &toc[1];
    assert_eq!(books.children.len(), 2);
    let numbers: Vec<i64> = books
        .children
        .iter()
        .flat_map(|t| &t.children)
        .map(|b| b.bible_book.unwrap())
        .collect();
    assert_eq!(numbers, (1..=66).collect::<Vec<_>>());
    assert_eq!(books.children[0].children.len(), 39);
    assert!(nwtsty.cover_image().unwrap().unwrap().ends_with("_cvr.jpg"));

    let wp_entry = lib.import(&wp).unwrap();
    let wp = jwlinux::Publication::open(&lib, &wp_entry).unwrap();
    let toc = wp.toc().unwrap();
    assert_eq!(toc.len(), 1);
    assert_eq!(toc[0].children.len(), 8);
    assert_eq!(toc[0].children[3].document_id, Some(3));
    assert!(toc[0].children.iter().all(|c| c.bible_book.is_none()));
    assert_eq!(
        wp.cover_image().unwrap().as_deref(),
        Some("2026000_X_cvr.jpg")
    );
}
