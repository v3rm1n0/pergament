use std::cell::RefCell;
use std::rc::Rc;

use lol_html::{RewriteStrSettings, doc_text, element, end_tag, rewrite_str};

use crate::{Error, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexedBlock {
    pub text: String,
    pub document: Option<i64>,
    pub paragraph: Option<i64>,
    pub book: Option<i64>,
    pub chapter: Option<i64>,
    pub verse: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ExtractedBlocks {
    pub blocks: Vec<IndexedBlock>,
    pub unanchored: u32,
}

#[derive(Debug, Default)]
struct ExtractionState {
    blocks: Vec<IndexedBlock>,
    active: Vec<BlockText>,
    unanchored: u32,
    skipped: u32,
    stray: Vec<bool>,
}

#[derive(Debug)]
struct BlockText {
    block: IndexedBlock,
    text: String,
}

fn decode_entities(text: &str) -> String {
    html_escape::decode_html_entities(text).to_string()
}

fn normalize_text(text: &str) -> String {
    decode_entities(text)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn parse_verse_id(id: &str) -> Option<(i64, i64, i64)> {
    let mut parts = id.strip_prefix('v')?.split('-');
    let book = parts.next()?.parse().ok()?;
    let chapter = parts.next()?.parse().ok()?;
    let verse = parts.next()?.parse().ok()?;
    Some((book, chapter, verse))
}

fn finish_block(state: &Rc<RefCell<ExtractionState>>) {
    let Some(mut block) = state.borrow_mut().active.pop() else {
        return;
    };
    block.text = normalize_text(&block.text);
    if !block.text.is_empty() {
        block.block.text = block.text;
        state.borrow_mut().blocks.push(block.block);
    }
}

fn extract(html: &str, selector: &'static str, block_for: impl Fn(&str) -> Option<IndexedBlock> + 'static) -> Result<ExtractedBlocks> {
    let state = Rc::new(RefCell::new(ExtractionState::default()));
    let element_state = Rc::clone(&state);
    let text_state = Rc::clone(&state);
    let skip_state = Rc::clone(&state);
    let settings = RewriteStrSettings::new()
        .append_element_content_handler(element!("style, script, span.cl, a.it, span.altsize, a.fn", move |el| {
            skip_state.borrow_mut().skipped += 1;
            let end_state = Rc::clone(&skip_state);
            let _ = el.on_end_tag(end_tag!(move |_| {
                end_state.borrow_mut().skipped -= 1;
                Ok(())
            }));
            Ok(())
        }))
        .append_element_content_handler(element!(selector, move |el| {
            let anchor = el
                .get_attribute("data-pid")
                .or_else(|| el.get_attribute("id"));
            if let Some(anchor) = anchor.as_deref().and_then(&block_for) {
                let end_state = Rc::clone(&element_state);
                element_state.borrow_mut().active.push(BlockText {
                    block: anchor,
                    text: String::new(),
                });
                let _ = el.on_end_tag(end_tag!(move |_| {
                    finish_block(&end_state);
                    Ok(())
                }));
            } else {
                element_state.borrow_mut().stray.push(false);
                let end_state = Rc::clone(&element_state);
                let _ = el.on_end_tag(end_tag!(move |_| {
                    let mut state = end_state.borrow_mut();
                    if state.stray.pop() == Some(true) {
                        state.unanchored += 1;
                    }
                    Ok(())
                }));
            }
            Ok(())
        }))
        .append_document_content_handler(doc_text!(move |text| {
            let mut state = text_state.borrow_mut();
            if state.skipped > 0 {
                return Ok(());
            }
            if let Some(block) = state.active.last_mut() {
                block.text.push_str(text.as_str());
            } else if !text.as_str().trim().is_empty() && let Some(stray) = state.stray.last_mut() {
                *stray = true;
            }
            Ok(())
        }));
    rewrite_str(html, settings).map_err(|error| Error::Decode(error.to_string()))?;
    let state = Rc::try_unwrap(state)
        .map_err(|_| Error::Decode("search extraction state still in use".into()))?
        .into_inner();
    Ok(ExtractedBlocks {
        blocks: state.blocks,
        unanchored: state.unanchored,
    })
}

pub fn extract_document_blocks(html: &str, document: i64) -> Result<ExtractedBlocks> {
    extract(html, "p, div", move |anchor| {
        anchor.parse().ok().map(|paragraph| IndexedBlock {
            text: String::new(),
            document: Some(document),
            paragraph: Some(paragraph),
            book: None,
            chapter: None,
            verse: None,
        })
    })
}

pub fn extract_chapter_blocks(html: &str, book: i64, chapter: i64) -> Result<ExtractedBlocks> {
    extract(html, "span.v", move |anchor| {
        let (anchor_book, anchor_chapter, verse) = parse_verse_id(anchor)?;
        (anchor_book == book && anchor_chapter == chapter).then_some(IndexedBlock {
            text: String::new(),
            document: None,
            paragraph: None,
            book: Some(book),
            chapter: Some(chapter),
            verse: Some(verse),
        })
    })
}
