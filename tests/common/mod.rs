//! Builders for tiny synthetic `.jwpub` files, valid and malicious.
#![allow(dead_code)]

use std::io::{Cursor, Write};
use std::path::Path;

use jwlinux::crypto::ContentKey;
use rusqlite::Connection;
use sha1::Sha1;
use sha2::{Digest, Sha256};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

pub const SYMBOL: &str = "tst";
pub const LANG: i64 = 0;
pub const YEAR: i64 = 2024;

pub fn key(issue: &str) -> ContentKey {
    ContentKey::new(LANG, SYMBOL, YEAR, issue)
}

/// Knobs for producing broken variants.
#[derive(Clone)]
pub struct Fixture {
    pub issue: String,
    pub title: String,
    pub with_db: bool,
    pub with_document_table: bool,
    pub with_manifest: bool,
    pub corrupt_hash: bool,
    pub corrupt_db_hash: bool,
    pub extra_entries: Vec<(String, Vec<u8>)>,
    pub db_file_name: Option<String>,
    pub documents: Vec<(i64, String)>,
}

impl Default for Fixture {
    fn default() -> Self {
        Self {
            issue: "0".into(),
            title: "Test Publication".into(),
            with_db: true,
            with_document_table: true,
            with_manifest: true,
            corrupt_hash: false,
            corrupt_db_hash: false,
            extra_entries: vec![("img_1.jpg".into(), b"\xff\xd8not really a jpeg".to_vec())],
            db_file_name: None,
            documents: vec![(1, "<p id=\"p1\" data-pid=\"1\">Hello</p>".into())],
        }
    }
}

impl Fixture {
    pub fn db_name(&self) -> String {
        self.db_file_name
            .clone()
            .unwrap_or_else(|| format!("{SYMBOL}_E.db"))
    }

    pub fn build_db(&self) -> Vec<u8> {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("x.db");
        let conn = Connection::open(&path).unwrap();
        conn.execute_batch(
            "CREATE TABLE Publication (PublicationId INTEGER PRIMARY KEY, Title TEXT, ShortTitle TEXT,
                 Symbol TEXT NOT NULL, Year INTEGER NOT NULL, MepsLanguageIndex INTEGER NOT NULL,
                 IssueTagNumber TEXT, PublicationType TEXT);",
        )
        .unwrap();
        conn.execute(
            "INSERT INTO Publication VALUES (1, ?1, 'Test', ?2, ?3, ?4, ?5, 'Book')",
            rusqlite::params![self.title, SYMBOL, YEAR, LANG, self.issue],
        )
        .unwrap();
        if self.with_document_table {
            conn.execute_batch(
                "CREATE TABLE Document (DocumentId INTEGER PRIMARY KEY, MepsDocumentId INTEGER,
                     Class TEXT, Type INTEGER, Title TEXT, Content BLOB, ContentLength INTEGER);",
            )
            .unwrap();
            let k = key(&self.issue);
            for (id, html) in &self.documents {
                conn.execute(
                    "INSERT INTO Document VALUES (?1, ?1, '13', 0, 'Doc', ?2, ?3)",
                    rusqlite::params![id, k.encode(html.as_bytes()), html.len() as i64],
                )
                .unwrap();
            }
        }
        drop(conn);
        std::fs::read(path).unwrap()
    }

    pub fn build(&self) -> Vec<u8> {
        let db = self.build_db();
        let mut entries: Vec<(String, Vec<u8>)> = Vec::new();
        if self.with_db {
            entries.push((self.db_name(), db.clone()));
        }
        entries.extend(self.extra_entries.iter().cloned());
        let contents = zip_bytes(&entries, CompressionMethod::Stored);

        let mut hash = hex::encode(Sha256::digest(&contents));
        if self.corrupt_hash {
            hash = hex::encode(Sha256::digest(b"something else"));
        }
        let mut db_hash = hex::encode(Sha1::digest(&db));
        if self.corrupt_db_hash {
            db_hash = hex::encode(Sha1::digest(b"something else"));
        }
        let manifest = serde_json::json!({
            "name": format!("{SYMBOL}_E.jwpub"),
            "hash": hash,
            "timestamp": "2024-01-01T00:00:00Z",
            "version": 1,
            "expandedSize": entries.iter().map(|(_, d)| d.len()).sum::<usize>(),
            "contentFormat": "z-a",
            "mepsPlatformVersion": "2.10",
            "publication": {
                "fileName": self.db_name(),
                "title": self.title,
                "symbol": SYMBOL,
                "language": LANG,
                "year": YEAR,
                "hash": db_hash,
            }
        });

        let mut outer: Vec<(String, Vec<u8>)> = Vec::new();
        if self.with_manifest {
            outer.push((
                "manifest.json".into(),
                serde_json::to_vec(&manifest).unwrap(),
            ));
        }
        outer.push(("contents".into(), contents));
        zip_bytes(&outer, CompressionMethod::Deflated)
    }

    pub fn write_to(&self, path: &Path) {
        std::fs::write(path, self.build()).unwrap();
    }
}

pub fn zip_bytes(entries: &[(String, Vec<u8>)], method: CompressionMethod) -> Vec<u8> {
    let mut zw = ZipWriter::new(Cursor::new(Vec::new()));
    let opts = SimpleFileOptions::default().compression_method(method);
    for (name, data) in entries {
        zw.start_file(name.as_str(), opts).unwrap();
        zw.write_all(data).unwrap();
    }
    zw.finish().unwrap().into_inner()
}
