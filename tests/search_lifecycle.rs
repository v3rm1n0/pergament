mod common;

use common::Fixture;
use pergament::library::Library;
use pergament::search::{CoverageState, SearchRequest, SearchScope};
use tempfile::tempdir;

fn request(query: &str) -> SearchRequest {
    SearchRequest {
        query: query.into(),
        scope: SearchScope::All,
        limit: None,
        offset: None,
    }
}

#[test]
fn library_mutations_work_when_search_is_unavailable() {
    let dir = tempdir().unwrap();
    let package = dir.path().join("fixture.jwpub");
    Fixture::default().write_to(&package);
    let root = dir.path().join("library");
    let mut library = Library::open(&root).unwrap();
    let connection = rusqlite::Connection::open(root.join("index.sqlite")).unwrap();
    connection.execute_batch("DROP TABLE search_fts; UPDATE search_meta SET available = 0;").unwrap();
    let entry = library.import(&package).unwrap();
    assert_eq!(library.search_status().unwrap().state, CoverageState::Unavailable);
    assert!(library.remove(&entry).unwrap());
    assert!(library.list().unwrap().is_empty());
}

#[test]
fn removing_a_publication_removes_its_search_hits() {
    let dir = tempdir().unwrap();
    let package = dir.path().join("fixture.jwpub");
    Fixture::default().write_to(&package);
    let mut library = Library::open(dir.path().join("library")).unwrap();
    let entry = library.import(&package).unwrap();
    library.rebuild_search().unwrap();
    assert_eq!(library.search(&request("hello")).unwrap().hits.len(), 1);

    assert!(library.remove(&entry).unwrap());

    let response = library.search(&request("hello")).unwrap();
    assert!(response.hits.is_empty());
    assert_eq!(response.status.total, 0);
    let connection = rusqlite::Connection::open(dir.path().join("library/index.sqlite")).unwrap();
    let orphaned_rows: u32 = connection
        .query_row(
            "SELECT COUNT(*) FROM search_fts WHERE dir_name = ?1",
            [&entry.dir_name],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(orphaned_rows, 0);
}

#[test]
fn replacing_a_publication_does_not_return_old_content() {
    let dir = tempdir().unwrap();
    let first = dir.path().join("first.jwpub");
    Fixture::default().write_to(&first);
    let second = dir.path().join("second.jwpub");
    Fixture {
        documents: vec![(1, r#"<p data-pid="1">Replacement text</p>"#.into())],
        ..Fixture::default()
    }
    .write_to(&second);
    let mut library = Library::open(dir.path().join("library")).unwrap();
    library.import(&first).unwrap();
    library.rebuild_search().unwrap();
    library.import(&second).unwrap();

    assert!(library.search(&request("hello")).unwrap().hits.is_empty());
    assert!(library.search(&request("replacement")).unwrap().hits.is_empty());
    library.rebuild_search().unwrap();
    assert_eq!(library.search(&request("replacement")).unwrap().hits.len(), 1);
}

#[test]
fn rebuild_indexes_a_preexisting_publication() {
    let dir = tempdir().unwrap();
    let package = dir.path().join("fixture.jwpub");
    Fixture::default().write_to(&package);
    let root = dir.path().join("library");
    let mut first = Library::open(&root).unwrap();
    first.import(&package).unwrap();
    drop(first);

    let mut reopened = Library::open(root).unwrap();
    assert!(reopened.search(&request("hello")).unwrap().hits.is_empty());
    let status = reopened.rebuild_search().unwrap();
    assert_eq!(status.indexed, 1);
    assert_eq!(reopened.search(&request("hello")).unwrap().hits.len(), 1);
}

#[test]
fn unanchored_content_keeps_coverage_incomplete() {
    let dir = tempdir().unwrap();
    let package = dir.path().join("fixture.jwpub");
    Fixture {
        documents: vec![(1, r#"<p data-pid="1">Anchored</p><div>Missing anchor</div>"#.into())],
        ..Fixture::default()
    }
    .write_to(&package);
    let mut library = Library::open(dir.path().join("library")).unwrap();
    library.import(&package).unwrap();
    let status = library.rebuild_search().unwrap();
    assert_eq!(status.unanchored, 1);
    assert_eq!(status.state, CoverageState::Incomplete);
}

#[test]
fn failed_indexing_does_not_discard_other_publication_and_retry_recovers() {
    let dir = tempdir().unwrap();
    let failed_package = dir.path().join("failed.jwpub");
    Fixture {
        documents: vec![(1, r#"<p data-pid="1">Failed publication text</p>"#.into())],
        ..Fixture::default()
    }
    .write_to(&failed_package);
    let good_package = dir.path().join("good.jwpub");
    Fixture {
        issue: "1".into(),
        documents: vec![(1, r#"<p data-pid="1">Good publication text</p>"#.into())],
        ..Fixture::default()
    }
    .write_to(&good_package);

    let root = dir.path().join("library");
    let mut library = Library::open(&root).unwrap();
    let failed_entry = library.import(&failed_package).unwrap();
    library.import(&good_package).unwrap();
    let failed_db = library.db_path(&failed_entry);
    let connection = rusqlite::Connection::open(failed_db).unwrap();
    connection
        .execute("UPDATE Document SET Content = X'00' WHERE DocumentId = 1", [])
        .unwrap();
    drop(connection);

    let status = library.rebuild_search().unwrap();
    assert_eq!(status.failed, 1);
    assert_eq!(status.indexed, 1);
    assert_eq!(status.state, CoverageState::Incomplete);
    assert_eq!(library.search(&request("good publication")).unwrap().hits.len(), 1);
    assert!(library.search(&request("failed publication")).unwrap().hits.is_empty());

    let repaired_package = dir.path().join("repaired.jwpub");
    Fixture {
        documents: vec![(1, r#"<p data-pid="1">Repaired publication text</p>"#.into())],
        ..Fixture::default()
    }
    .write_to(&repaired_package);
    library.import(&repaired_package).unwrap();
    let status = library.rebuild_search().unwrap();
    assert_eq!(status.failed, 0);
    assert_eq!(status.state, CoverageState::Ready);
    assert_eq!(library.search(&request("repaired publication")).unwrap().hits.len(), 1);
}

#[test]
fn failed_coverage_survives_restart() {
    let dir = tempdir().unwrap();
    let package = dir.path().join("fixture.jwpub");
    Fixture::default().write_to(&package);
    let root = dir.path().join("library");
    let mut library = Library::open(&root).unwrap();
    let entry = library.import(&package).unwrap();
    let database = library.db_path(&entry);
    let connection = rusqlite::Connection::open(database).unwrap();
    connection
        .execute("UPDATE Document SET Content = X'00' WHERE DocumentId = 1", [])
        .unwrap();
    drop(connection);
    let failed = library.rebuild_search().unwrap();
    assert_eq!(failed.failed, 1);
    drop(library);

    let mut reopened = Library::open(&root).unwrap();
    let persisted = reopened.search_status().unwrap();
    assert_eq!(persisted.failed, 1);
    assert_eq!(persisted.state, CoverageState::Incomplete);
    let recovered = reopened.rebuild_search().unwrap();
    assert_eq!(recovered.failed, 1);
}

#[test]
fn rebuild_removes_orphaned_search_rows() {
    let dir = tempdir().unwrap();
    let root = dir.path().join("library");
    let mut library = Library::open(&root).unwrap();
    let connection = rusqlite::Connection::open(root.join("index.sqlite")).unwrap();
    connection
        .execute(
            "INSERT INTO search_fts (text, dir_name, document_id, paragraph_id, title, publication_title, is_bible)
             VALUES ('orphan text', 'missing-publication', 1, 1, 'Orphan', 'Orphan', 0)",
            [],
        )
        .unwrap();
    drop(connection);

    library.rebuild_search().unwrap();

    let connection = rusqlite::Connection::open(root.join("index.sqlite")).unwrap();
    let rows: u32 = connection
        .query_row(
            "SELECT COUNT(*) FROM search_fts WHERE dir_name = 'missing-publication'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(rows, 0);
}
