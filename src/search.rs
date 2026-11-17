use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};

use crate::library::{Entry, Library};
use crate::navigate::{Target, TargetKind};
use crate::reader::Publication;
use crate::search_index::IndexedBlock;
use crate::{Error, Result};

const SCHEMA_VERSION: i64 = 1;
const DEFAULT_LIMIT: u32 = 50;
const MAX_LIMIT: u32 = 100;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum SearchScope {
    All,
    Bibles,
    Publication {
        #[serde(rename = "dirName")]
        dir_name: String,
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchRequest {
    pub query: String,
    pub scope: SearchScope,
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CoverageState {
    Ready,
    Indexing,
    Incomplete,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchStatus {
    pub state: CoverageState,
    pub total: u32,
    pub indexed: u32,
    pub failed: u32,
    pub unanchored: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchHit {
    pub title: String,
    pub publication_title: String,
    pub snippet: String,
    pub target: Target,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchResponse {
    pub hits: Vec<SearchHit>,
    pub status: SearchStatus,
}

pub(crate) fn initialize(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS search_meta (
             schema_version INTEGER NOT NULL,
             available INTEGER NOT NULL
         );
         CREATE TABLE IF NOT EXISTS search_coverage (
             dir_name TEXT PRIMARY KEY NOT NULL,
             contents_hash TEXT NOT NULL,
             state TEXT NOT NULL,
             failed INTEGER NOT NULL DEFAULT 0,
             unanchored INTEGER NOT NULL DEFAULT 0
         );",
    )?;
    conn.execute(
        "INSERT INTO search_meta (schema_version, available)
         SELECT ?1, 1
         WHERE NOT EXISTS (SELECT 1 FROM search_meta)",
        [SCHEMA_VERSION],
    )?;
    let available: i64 = conn.query_row("SELECT available FROM search_meta LIMIT 1", [], |r| r.get(0))?;
    if available != 0 {
        let fts = conn.execute_batch(
            "CREATE VIRTUAL TABLE IF NOT EXISTS search_fts USING fts5(
                 text,
                 dir_name UNINDEXED,
                 document_id UNINDEXED,
                 paragraph_id UNINDEXED,
                 book UNINDEXED,
                 chapter UNINDEXED,
                 verse UNINDEXED,
                 title UNINDEXED,
                 publication_title UNINDEXED,
                 is_bible UNINDEXED,
                 tokenize = 'unicode61 remove_diacritics 2'
             );",
        );
        if fts.is_err() {
            conn.execute("UPDATE search_meta SET available = 0", [])?;
        }
    }
    Ok(())
}

pub(crate) fn status(conn: &Connection) -> Result<SearchStatus> {
    let schema_version: Option<i64> = conn
        .query_row("SELECT schema_version FROM search_meta LIMIT 1", [], |r| r.get(0))
        .optional()?;
    let available = match schema_version {
        Some(v) if v == SCHEMA_VERSION => conn
            .query_row("SELECT available FROM search_meta LIMIT 1", [], |r| r.get::<_, i64>(0))
            .optional()?
            .unwrap_or(0),
        _ => 0,
    };
    let total: u32 = conn.query_row("SELECT COUNT(*) FROM publication", [], |r| r.get(0))?;
    if available == 0 {
        return Ok(SearchStatus { state: CoverageState::Unavailable, total, indexed: 0, failed: 0, unanchored: 0 });
    }
    let indexed: u32 = conn.query_row(
        "SELECT COUNT(*) FROM publication p JOIN search_coverage c ON c.dir_name = p.dir_name
         WHERE c.state = 'ready' AND c.contents_hash = p.contents_hash",
        [], |r| r.get(0),
    )?;
    let failed: u32 = conn.query_row("SELECT COALESCE(SUM(failed), 0) FROM search_coverage", [], |r| r.get(0))?;
    let unanchored: u32 = conn.query_row("SELECT COALESCE(SUM(unanchored), 0) FROM search_coverage", [], |r| r.get(0))?;
    let indexing: bool = conn.query_row(
        "SELECT EXISTS (SELECT 1 FROM search_coverage WHERE state = 'indexing')",
        [],
        |r| r.get(0),
    )?;
    let state = if indexing {
        CoverageState::Indexing
    } else if total == indexed && failed == 0 && unanchored == 0 {
        CoverageState::Ready
    } else {
        CoverageState::Incomplete
    };
    Ok(SearchStatus { state, total, indexed, failed, unanchored })
}

pub(crate) fn search(conn: &Connection, request: &SearchRequest) -> Result<SearchResponse> {
    let status = status(conn)?;
    if request.query.trim().is_empty() || status.state == CoverageState::Unavailable {
        return Ok(SearchResponse { hits: Vec::new(), status });
    }
    let query = fts_query(&request.query)?;
    let limit = request.limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT);
    let offset = request.offset.unwrap_or(0);
    let mut sql = String::from(
        "SELECT title, publication_title, dir_name, document_id, paragraph_id, book, chapter, verse,
                snippet(search_fts, 0, '', '', ' … ', 24)
         FROM search_fts
         WHERE search_fts MATCH ?1
           AND EXISTS (SELECT 1 FROM publication p JOIN search_coverage c ON c.dir_name = p.dir_name
                       WHERE p.dir_name = search_fts.dir_name AND c.state = 'ready'
                         AND c.contents_hash = p.contents_hash)",
    );
    sql.push_str(" AND (?2 IS NULL OR dir_name = ?2)");
    if matches!(request.scope, SearchScope::Bibles) {
        sql.push_str(" AND is_bible = 1");
    }
    sql.push_str(" ORDER BY bm25(search_fts), dir_name, document_id, paragraph_id, book, chapter, verse LIMIT ?3 OFFSET ?4");
    let publication = match &request.scope {
        SearchScope::Publication { dir_name } => Some(dir_name.as_str()),
        _ => None,
    };
    let mut stmt = conn.prepare(&sql)?;
    let mut rows = stmt.query(params![query, publication, limit, offset])?;
    let mut hits = Vec::new();
    while let Some(row) = rows.next()? {
        let dir_name: String = row.get(2)?;
        let document: Option<i64> = row.get(3)?;
        let paragraph: Option<i64> = row.get(4)?;
        let book: Option<i64> = row.get(5)?;
        let chapter: Option<i64> = row.get(6)?;
        let verse: Option<i64> = row.get(7)?;
        let kind = match (document, paragraph, book, chapter, verse) {
            (Some(document), Some(paragraph), _, _, _) => TargetKind::DocumentParagraph { document, paragraph },
            (_, _, Some(book), Some(chapter), Some(verse)) => TargetKind::Chapter { book, chapter, verse },
            _ => continue,
        };
        hits.push(SearchHit {
            title: row.get(0)?,
            publication_title: row.get(1)?,
            snippet: row.get(8)?,
            target: Target { publication: dir_name, kind },
        });
    }
    Ok(SearchResponse { hits, status })
}

fn fts_query(input: &str) -> Result<String> {
    let quote_count = input.chars().filter(|c| *c == '"').count();
    if quote_count % 2 != 0 {
        return Err(Error::InvalidQuery("unmatched quote".into()));
    }
    let mut query = String::new();
    let mut phrase = false;
    let mut token = String::new();
    for ch in input.chars() {
        match ch {
            '"' => {
                if !token.trim().is_empty() {
                    if !query.is_empty() { query.push_str(" AND "); }
                    query.push('"');
                    query.push_str(&token.replace('"', "\"\""));
                    query.push('"');
                    token.clear();
                }
                phrase = !phrase;
            }
            c if c.is_whitespace() && !phrase => {
                if !token.trim().is_empty() {
                    if !query.is_empty() { query.push_str(" AND "); }
                    query.push('"');
                    query.push_str(&token.replace('"', "\"\""));
                    query.push('"');
                    token.clear();
                }
            }
            _ => token.push(ch),
        }
    }
    if !token.trim().is_empty() {
        if !query.is_empty() { query.push_str(" AND "); }
        query.push('"');
        query.push_str(&token.replace('"', "\"\""));
        query.push('"');
    }
    if query.is_empty() { return Err(Error::InvalidQuery("empty search query".into())); }
    Ok(query)
}

pub(crate) fn begin_rebuild(conn: &Connection) -> Result<()> {
    conn.execute(
        "INSERT INTO search_coverage (dir_name, contents_hash, state, failed, unanchored)
         SELECT dir_name, contents_hash, 'indexing', 0, 0
         FROM publication
         WHERE NOT EXISTS (
             SELECT 1 FROM search_coverage c WHERE c.dir_name = publication.dir_name
         )",
        [],
    )?;
    conn.execute(
        "UPDATE search_coverage
         SET state = 'indexing', failed = 0, unanchored = 0
         WHERE dir_name IN (SELECT dir_name FROM publication)
           AND NOT (
               state = 'ready'
               AND contents_hash = (SELECT p.contents_hash FROM publication p WHERE p.dir_name = search_coverage.dir_name)
           )",
        [],
    )?;
    Ok(())
}

pub(crate) fn is_ready(conn: &Connection, entry: &Entry) -> Result<bool> {
    let row: Option<(String, String)> = conn
        .query_row(
            "SELECT state, contents_hash FROM search_coverage WHERE dir_name = ?1",
            [&entry.dir_name],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    Ok(matches!(row, Some((s, h)) if s == "ready" && h == entry.contents_hash))
}

pub(crate) fn rebuild(conn: &Connection) -> Result<SearchStatus> {
    if !available(conn)? {
        return status(conn);
    }
    let tx = conn.unchecked_transaction()?;
    tx.execute(
        "DELETE FROM search_fts
         WHERE NOT EXISTS (SELECT 1 FROM publication p WHERE p.dir_name = search_fts.dir_name)",
        [],
    )?;
    tx.execute(
        "DELETE FROM search_coverage
         WHERE NOT EXISTS (SELECT 1 FROM publication p WHERE p.dir_name = search_coverage.dir_name)",
        [],
    )?;
    tx.commit()?;
    status(conn)
}

pub(crate) fn available(conn: &Connection) -> Result<bool> {
    Ok(conn.query_row("SELECT available FROM search_meta LIMIT 1", [], |row| row.get::<_, i64>(0)).optional()?.unwrap_or(0) != 0)
}

pub(crate) fn invalidate_publication(conn: &Connection, entry: &Entry) -> Result<()> {
    if !available(conn)? {
        return Ok(());
    }
    conn.execute("DELETE FROM search_fts WHERE dir_name = ?1", [&entry.dir_name])?;
    conn.execute(
        "INSERT INTO search_coverage (dir_name, contents_hash, state, failed, unanchored)
         VALUES (?1, ?2, 'pending', 0, 0)
         ON CONFLICT(dir_name) DO UPDATE SET contents_hash = excluded.contents_hash, state = 'pending', failed = 0, unanchored = 0",
        params![entry.dir_name, entry.contents_hash],
    )?;
    Ok(())
}

pub(crate) fn index_publication(conn: &Connection, library: &Library, entry: &Entry) -> Result<()> {
    let publication = Publication::open(library, entry)?;
    let mut blocks = Vec::new();
    let mut unanchored = 0u32;
    for document in publication.documents()? {
        if let Some(html) = publication.document(document.id)?.1 {
            let extracted = crate::search_index::extract_document_blocks(&html, document.id)?;
            unanchored += extracted.unanchored;
            blocks.extend(extracted.blocks.into_iter().map(|block| (block, document.title.clone())));
        }
    }
    if publication.is_bible() {
        for book in publication.bible_books()? {
            for chapter in 1..=book.chapters {
                let content = publication.chapter(book.number, chapter)?;
                let extracted = crate::search_index::extract_chapter_blocks(&content.content, book.number, chapter)?;
                unanchored += extracted.unanchored;
                let title = format!("{} {chapter}", book.chapter_title);
                blocks.extend(extracted.blocks.into_iter().map(|block| (block, title.clone())));
            }
        }
    }
    commit_index(conn, entry, blocks, unanchored)
}

fn commit_index(
    conn: &Connection,
    entry: &Entry,
    blocks: Vec<(IndexedBlock, String)>,
    unanchored: u32,
) -> Result<()> {
    let tx = conn.unchecked_transaction()?;
    let authoritative_hash: Option<String> = tx
        .query_row(
            "SELECT contents_hash FROM publication WHERE dir_name = ?1",
            [&entry.dir_name],
            |row| row.get(0),
        )
        .optional()?;
    if authoritative_hash.as_ref().is_some_and(|hash| hash != &entry.contents_hash) {
        tx.commit()?;
        return Ok(());
    }
    tx.execute("DELETE FROM search_fts WHERE dir_name = ?1", [&entry.dir_name])?;
    tx.execute("DELETE FROM search_coverage WHERE dir_name = ?1", [&entry.dir_name])?;
    if authoritative_hash.is_none() {
        tx.commit()?;
        return Ok(());
    }
    for (block, title) in blocks {
        tx.execute(
            "INSERT INTO search_fts (text, dir_name, document_id, paragraph_id, book, chapter, verse, title, publication_title, is_bible)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![block.text, entry.dir_name, block.document, block.paragraph, block.book, block.chapter, block.verse, title, entry.title, block.book.is_some()],
        )?;
    }
    tx.execute(
        "INSERT INTO search_coverage (dir_name, contents_hash, state, failed, unanchored)
         VALUES (?1, ?2, 'ready', 0, ?3)",
        params![entry.dir_name, entry.contents_hash, unanchored],
    )?;
    tx.commit()?;
    Ok(())
}

pub(crate) fn index_error(conn: &Connection, entry: &Entry) -> Result<()> {
    conn.execute(
        "UPDATE search_coverage
         SET state = 'failed', failed = 1
         WHERE dir_name = ?1 AND contents_hash = ?2",
        params![entry.dir_name, entry.contents_hash],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn beginning_rebuild_keeps_ready_publications_searchable() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("CREATE TABLE publication (dir_name TEXT PRIMARY KEY, contents_hash TEXT NOT NULL); INSERT INTO publication VALUES ('ready', 'hash'), ('new', 'hash');").unwrap();
        initialize(&conn).unwrap();
        conn.execute("INSERT INTO search_coverage VALUES ('ready', 'hash', 'ready', 0, 0)", []).unwrap();
        conn.execute("INSERT INTO search_fts (text, dir_name, document_id, paragraph_id, title, publication_title) VALUES ('hello', 'ready', 1, 1, 'Title', 'Publication')", []).unwrap();
        begin_rebuild(&conn).unwrap();
        let result = search(&conn, &SearchRequest { query: "hello".into(), scope: SearchScope::All, limit: None, offset: None }).unwrap();
        assert_eq!(result.status.state, CoverageState::Indexing);
        assert_eq!(result.hits.len(), 1);
    }

    #[test]
    fn a_failure_for_a_stale_snapshot_leaves_the_ready_coverage_alone() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute(
            "CREATE TABLE publication (dir_name TEXT PRIMARY KEY, contents_hash TEXT NOT NULL)",
            [],
        )
        .unwrap();
        initialize(&conn).unwrap();
        conn.execute(
            "INSERT INTO publication (dir_name, contents_hash) VALUES ('test', 'current')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO search_coverage VALUES ('test', 'current', 'ready', 0, 0)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO search_fts (text, dir_name, document_id, paragraph_id, title, publication_title)
             VALUES ('hello', 'test', 1, 1, 'Doc', 'Pub')",
            [],
        )
        .unwrap();
        let entry = Entry {
            symbol: "tst".into(),
            meps_language: 0,
            issue_tag: "0".into(),
            year: 2024,
            title: "Test".into(),
            short_title: None,
            publication_type: None,
            dir_name: "test".into(),
            db_file: "tst_E.db".into(),
            contents_hash: "stale".into(),
            imported_at: 0,
            lang_code: None,
            package_sha1: None,
        };

        index_error(&conn, &entry).unwrap();

        let status = status(&conn).unwrap();
        assert_eq!(status.state, CoverageState::Ready);
        assert_eq!(status.failed, 0);
        assert_eq!(
            status.indexed, 1
        );
    }

    #[test]
    fn stale_commit_preserves_newer_search_results() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute(
            "CREATE TABLE publication (dir_name TEXT PRIMARY KEY, contents_hash TEXT NOT NULL)",
            [],
        )
        .unwrap();
        initialize(&conn).unwrap();
        conn.execute(
            "INSERT INTO publication (dir_name, contents_hash) VALUES ('test', 'current')",
            [],
        )
        .unwrap();
        conn.execute("INSERT INTO search_coverage VALUES ('test', 'current', 'ready', 0, 0)", []).unwrap();
        conn.execute("INSERT INTO search_fts (text, dir_name, document_id, paragraph_id, title, publication_title) VALUES ('newer', 'test', 1, 1, 'Doc', 'Pub')", []).unwrap();
        let entry = Entry {
            symbol: "tst".into(), meps_language: 0, issue_tag: "0".into(), year: 2024,
            title: "Test".into(), short_title: None, publication_type: None,
            dir_name: "test".into(), db_file: "tst_E.db".into(),
            contents_hash: "stale".into(), imported_at: 0, lang_code: None, package_sha1: None,
        };

        commit_index(&conn, &entry, vec![], 0).unwrap();

        let result = search(&conn, &SearchRequest { query: "newer".into(), scope: SearchScope::All, limit: None, offset: None }).unwrap();
        assert_eq!(result.status.state, CoverageState::Ready);
        assert_eq!(result.hits.len(), 1);
    }

    #[test]
    fn a_newer_schema_version_is_reported_as_unavailable() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute(
            "CREATE TABLE publication (dir_name TEXT PRIMARY KEY, contents_hash TEXT NOT NULL)",
            [],
        )
        .unwrap();
        initialize(&conn).unwrap();
        conn.execute("UPDATE search_meta SET schema_version = 2", []).unwrap();

        let status = status(&conn).unwrap();

        assert_eq!(status.state, CoverageState::Unavailable);
    }

    #[test]
    fn stale_snapshot_does_not_publish_ready_coverage() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute(
            "CREATE TABLE publication (dir_name TEXT PRIMARY KEY, contents_hash TEXT NOT NULL)",
            [],
        )
        .unwrap();
        initialize(&conn).unwrap();
        conn.execute(
            "INSERT INTO publication (dir_name, contents_hash) VALUES ('test', 'current')",
            [],
        )
        .unwrap();
        let entry = Entry {
            symbol: "tst".into(),
            meps_language: 0,
            issue_tag: "0".into(),
            year: 2024,
            title: "Test".into(),
            short_title: None,
            publication_type: None,
            dir_name: "test".into(),
            db_file: "tst_E.db".into(),
            contents_hash: "stale".into(),
            imported_at: 0,
            lang_code: None,
            package_sha1: None,
        };
        let block = IndexedBlock {
            text: "stale text".into(),
            document: Some(1),
            paragraph: Some(1),
            book: None,
            chapter: None,
            verse: None,
        };

        commit_index(&conn, &entry, vec![(block, "Doc".into())], 0).unwrap();

        let status = status(&conn).unwrap();
        assert_eq!(status.indexed, 0);
        assert_eq!(status.state, CoverageState::Incomplete);
        let rows: u32 = conn
            .query_row("SELECT COUNT(*) FROM search_fts", [], |row| row.get(0))
            .unwrap();
        assert_eq!(rows, 0);
    }
}

