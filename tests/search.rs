mod common;

use common::Fixture;
use pergament::library::Library;
use pergament::navigate::TargetKind;
use pergament::search::{SearchRequest, SearchScope};
use pergament::search_index::{extract_chapter_blocks, extract_document_blocks};
use tempfile::tempdir;

#[test]
fn opens_existing_library_and_reports_unindexed_publication() {
    let dir = tempdir().unwrap();
    let package = dir.path().join("fixture.jwpub");
    Fixture::default().write_to(&package);
    let root = dir.path().join("library");
    let mut library = Library::open(&root).unwrap();
    library.import(&package).unwrap();
    drop(library);

    let library = Library::open(root).unwrap();
    let status = library.search_status().unwrap();
    assert_eq!(status.state, pergament::search::CoverageState::Incomplete);
    assert_eq!(status.total, 1);
    assert_eq!(status.indexed, 0);
    assert_eq!(status.failed, 0);
}

#[test]
fn search_schema_survives_reopening() {
    let dir = tempdir().unwrap();
    let root = dir.path();

    let library = Library::open(root).unwrap();
    let first = library.search_status().unwrap();
    drop(library);

    let library = Library::open(root).unwrap();
    let second = library.search_status().unwrap();
    assert_eq!(first, second);
}

#[test]
fn searches_imported_paragraphs_and_returns_exact_target() {
    let dir = tempdir().unwrap();
    let package = dir.path().join("fixture.jwpub");
    Fixture {
        documents: vec![(
            1,
            r#"<p data-pid="12">Café search phrase</p><p data-pid="13">other words</p>"#.into(),
        )],
        ..Fixture::default()
    }
    .write_to(&package);
    let mut library = Library::open(dir.path().join("library")).unwrap();
    let entry = library.import(&package).unwrap();
    library.rebuild_search().unwrap();

    let response = library
        .search(&SearchRequest {
            query: "cafe phrase".into(),
            scope: SearchScope::All,
            limit: None,
            offset: None,
        })
        .unwrap();

    assert_eq!(response.hits.len(), 1);
    assert_eq!(response.hits[0].publication_title, "Test Publication");
    assert_eq!(response.hits[0].snippet, "Café search phrase");
    assert_eq!(response.hits[0].target.publication, entry.dir_name);
    assert_eq!(
        response.hits[0].target.kind,
        TargetKind::DocumentParagraph {
            document: 1,
            paragraph: 12,
        }
    );
}

#[test]
fn searches_bible_verses_with_exact_targets_and_bible_scope() {
    let dir = tempdir().unwrap();
    let package = dir.path().join("bible.jwpub");
    Fixture {
        documents: vec![],
        bible_chapters: vec![(1, 1, r#"<p><span class="v" id="v1-1-1-1">Beginning heavens</span><span class="v" id="v1-1-2-1">Earth waters</span></p>"#.into())],
        ..Fixture::default()
    }.write_to(&package);
    let mut library = Library::open(dir.path().join("library")).unwrap();
    let entry = library.import(&package).unwrap();
    library.rebuild_search().unwrap();
    let result = library.search(&SearchRequest { query: "beginning".into(), scope: SearchScope::Bibles, limit: None, offset: None }).unwrap();
    assert_eq!(result.hits.len(), 1);
    assert_eq!(result.hits[0].target.publication, entry.dir_name);
    assert_eq!(result.hits[0].target.kind, TargetKind::Chapter { book: 1, chapter: 1, verse: 1 });
}

#[test]
fn search_rejects_unmatched_phrase_quote() {
    let dir = tempdir().unwrap();
    let library = Library::open(dir.path()).unwrap();
    let error = library
        .search(&SearchRequest {
            query: "\"unfinished".into(),
            scope: SearchScope::All,
            limit: None,
            offset: None,
        })
        .unwrap_err();
    assert!(matches!(error, pergament::Error::InvalidQuery(_)));
}

#[test]
fn punctuation_only_query_matches_nothing_without_error() {
    let dir = tempdir().unwrap();
    let library = Library::open(dir.path()).unwrap();
    let response = library
        .search(&SearchRequest {
            query: "-- !!".into(),
            scope: SearchScope::All,
            limit: None,
            offset: None,
        })
        .unwrap();
    assert!(response.hits.is_empty());
}

#[test]
fn extracts_anchored_paragraphs_as_plain_text() {
    let html = r#"<style>hidden</style><script>also hidden</script>
        <p data-pid="12">First <strong>paragraph</strong> &amp; text</p>
        <div>unanchored text</div>
        <p data-pid="13">Second <em>paragraph</em></p>"#;

    let result = extract_document_blocks(html, 7).unwrap();

    assert_eq!(result.unanchored, 1);
    assert_eq!(
        result.blocks,
        vec![
            pergament::search_index::IndexedBlock {
                text: "First paragraph & text".into(),
                document: Some(7),
                paragraph: Some(12),
                book: None,
                chapter: None,
                verse: None,
            },
            pergament::search_index::IndexedBlock {
                text: "Second paragraph".into(),
                document: Some(7),
                paragraph: Some(13),
                book: None,
                chapter: None,
                verse: None,
            },
        ]
    );
}

#[test]
fn extracts_bible_verse_anchors_including_verse_one() {
    let html = r#"<p><span id="v1-2-1-1" class="v">One <strong>text</strong></span>
        <span id="v1-2-2-1" class="v">Two</span></p>"#;

    let result = extract_chapter_blocks(html, 1, 2).unwrap();

    assert_eq!(result.unanchored, 0);
    assert_eq!(result.blocks[0].verse, Some(1));
    assert_eq!(result.blocks[0].text, "One text");
    assert_eq!(result.blocks[1].verse, Some(2));
    assert_eq!(result.blocks[1].text, "Two");
}
