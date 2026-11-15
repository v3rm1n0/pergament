use pergament::search_index::{extract_chapter_blocks, extract_document_blocks, IndexedBlock};

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

#[test]
fn strips_style_script_and_decodes_entities() {
    let html = r#"<p data-pid="1">Hello<style>.x{color:red}</style><script>alert(1)</script> world&nbsp;foo&mdash;bar&hellip;baz</p>"#;

    let result = extract_document_blocks(html, 1).unwrap();

    assert_eq!(
        result.blocks,
        vec![IndexedBlock {
            text: "Hello world foo—bar…baz".into(),
            document: Some(1),
            paragraph: Some(1),
            book: None,
            chapter: None,
            verse: None,
        }]
    );
}

#[test]
fn wrapper_divs_around_anchored_content_are_not_counted_as_unanchored() {
    let html = r#"<div><p data-pid="1">Hello</p><div></div></div>"#;

    let result = extract_document_blocks(html, 1).unwrap();

    assert_eq!(result.unanchored, 0);
    assert_eq!(
        result.blocks,
        vec![IndexedBlock {
            text: "Hello".into(),
            document: Some(1),
            paragraph: Some(1),
            book: None,
            chapter: None,
            verse: None,
        }]
    );
}

#[test]
fn strips_markers_and_footnotes() {
    let html = r#"<p data-pid="1">Hello<span class="cl"> marker </span> world<a class="fn">1</a> foo<a class="it">xref</a> bar</p>"#;

    let result = extract_document_blocks(html, 1).unwrap();

    assert_eq!(
        result.blocks,
        vec![IndexedBlock {
            text: "Hello world foo bar".into(),
            document: Some(1),
            paragraph: Some(1),
            book: None,
            chapter: None,
            verse: None,
        }]
    );
}
