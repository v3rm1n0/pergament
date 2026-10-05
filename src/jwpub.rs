//! Reading and safely unpacking `.jwpub` archives. All input is untrusted.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Seek, Write};
use std::path::{Component, Path, PathBuf};

use rusqlite::{Connection, OpenFlags, OptionalExtension};
use sha1::Sha1;
use sha2::{Digest, Sha256};
use zip::ZipArchive;

use crate::manifest::{MAX_MANIFEST_SIZE, Manifest};
use crate::{Error, Result};

const MANIFEST: &str = "manifest.json";
const CONTENTS: &str = "contents";

/// Limits applied while unpacking. The defaults are far above any real
/// publication (the study Bible expands to ~150 MB / 721 files).
#[derive(Debug, Clone, Copy)]
pub struct Limits {
    pub max_contents_size: u64,
    pub max_expanded_size: u64,
    pub max_entries: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_contents_size: 4 << 30,
            max_expanded_size: 8 << 30,
            max_entries: 100_000,
        }
    }
}

/// An opened `.jwpub` whose `contents` hash has been verified.
#[derive(Debug)]
pub struct JwPub {
    pub manifest: Manifest,
    contents: File,
    limits: Limits,
}

impl JwPub {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        Self::open_with_limits(path, Limits::default())
    }

    pub fn open_with_limits(path: impl AsRef<Path>, limits: Limits) -> Result<Self> {
        Self::from_reader(File::open(path)?, limits)
    }

    pub fn from_reader<R: Read + Seek>(reader: R, limits: Limits) -> Result<Self> {
        let mut outer = ZipArchive::new(reader)?;

        let manifest = {
            let entry = outer
                .by_name(MANIFEST)
                .map_err(|_| Error::MissingEntry(MANIFEST.into()))?;
            let mut buf = Vec::new();
            entry.take(MAX_MANIFEST_SIZE + 1).read_to_end(&mut buf)?;
            if buf.len() as u64 > MAX_MANIFEST_SIZE {
                return Err(Error::TooLarge("manifest.json".into()));
            }
            Manifest::parse(&buf)?
        };

        // Spool `contents` to an anonymous temp file while hashing it, so large
        // publications never have to sit in memory.
        let mut contents = tempfile::tempfile()?;
        let actual = {
            let entry = outer
                .by_name(CONTENTS)
                .map_err(|_| Error::MissingEntry(CONTENTS.into()))?;
            let mut hasher = HashingWriter::new(&mut contents, Sha256::new());
            let n = io::copy(&mut entry.take(limits.max_contents_size + 1), &mut hasher)?;
            if n > limits.max_contents_size {
                return Err(Error::TooLarge(format!(
                    "contents > {} bytes",
                    limits.max_contents_size
                )));
            }
            hex::encode(hasher.finish())
        };
        check_hash("contents SHA-256", &manifest.hash, &actual)?;
        contents.rewind()?;

        Ok(Self {
            manifest,
            contents,
            limits,
        })
    }

    /// Unpack `contents` into `dest`, which must be an existing empty
    /// directory. Rejects path traversal, links, duplicates and oversize input.
    pub fn extract_to(&mut self, dest: &Path) -> Result<()> {
        self.contents.rewind()?;
        let mut inner = ZipArchive::new(&mut self.contents)?;
        if inner.len() > self.limits.max_entries {
            return Err(Error::TooLarge(format!("{} entries", inner.len())));
        }
        let mut total: u64 = 0;
        for i in 0..inner.len() {
            let mut entry = inner.by_index(i)?;
            let name = entry.name().to_owned();
            if entry.is_symlink() {
                return Err(Error::UnsafePath(name));
            }
            if entry.is_dir() {
                safe_relative_path(name.trim_end_matches('/'))?;
                continue;
            }
            let rel = safe_relative_path(&name)?;
            let target = dest.join(&rel);
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent)?;
            }
            // create_new: never follow or overwrite anything already there,
            // which also rejects duplicate names.
            let mut out = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&target)
                .map_err(|e| match e.kind() {
                    io::ErrorKind::AlreadyExists => Error::UnsafePath(name.clone()),
                    _ => Error::Io(e),
                })?;
            let remaining = self.limits.max_expanded_size - total;
            let n = io::copy(&mut (&mut entry).take(remaining + 1), &mut out)?;
            total += n;
            if total > self.limits.max_expanded_size {
                return Err(Error::TooLarge(format!(
                    "expanded size > {} bytes",
                    self.limits.max_expanded_size
                )));
            }
        }
        Ok(())
    }

    /// File name of the publication database, checked to be a plain name.
    pub fn db_file_name(&self) -> Result<&str> {
        let name = &self.manifest.publication.file_name;
        let p = Path::new(name);
        if p.components().count() != 1
            || !matches!(p.components().next(), Some(Component::Normal(_)))
        {
            return Err(Error::UnsafePath(name.clone()));
        }
        Ok(name)
    }
}

/// Validate an archive entry name and turn it into a relative path.
fn safe_relative_path(name: &str) -> Result<PathBuf> {
    let bad = || Error::UnsafePath(name.to_owned());
    if name.is_empty() || name.contains('\\') || name.contains('\0') {
        return Err(bad());
    }
    let mut out = PathBuf::new();
    for part in name.split('/') {
        if part.is_empty() || part == "." || part == ".." || part.contains(':') {
            return Err(bad());
        }
        out.push(part);
    }
    // Defence in depth: the result must only contain normal components.
    if !out.components().all(|c| matches!(c, Component::Normal(_))) {
        return Err(bad());
    }
    Ok(out)
}

fn check_hash(what: &'static str, expected: &str, actual: &str) -> Result<()> {
    if expected.trim().eq_ignore_ascii_case(actual) {
        Ok(())
    } else {
        Err(Error::HashMismatch {
            what,
            expected: expected.to_owned(),
            actual: actual.to_owned(),
        })
    }
}

/// Verify the SHA-1 of the database against `publication.hash`, if present.
pub fn verify_db_hash(manifest: &Manifest, db: &Path) -> Result<()> {
    let Some(expected) = manifest.publication.hash.as_deref() else {
        return Ok(());
    };
    let mut hasher = HashingWriter::new(io::sink(), Sha1::new());
    io::copy(&mut File::open(db)?, &mut hasher)?;
    check_hash("database SHA-1", expected, &hex::encode(hasher.finish()))
}

/// Identity and titles read from the `Publication` table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublicationInfo {
    pub meps_language: i64,
    pub symbol: String,
    pub year: i64,
    pub issue_tag: String,
    pub title: String,
    pub short_title: Option<String>,
    pub publication_type: Option<String>,
}

/// Open a publication database read-only with hardening for untrusted files.
pub fn open_db(path: &Path) -> Result<Connection> {
    let conn = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    conn.execute_batch("PRAGMA trusted_schema = OFF; PRAGMA cell_size_check = ON;")?;
    Ok(conn)
}

/// Check that `path` is a publication database and read its identity.
pub fn inspect_db(path: &Path) -> Result<PublicationInfo> {
    let invalid = |reason: String| Error::InvalidDatabase {
        path: path.to_owned(),
        reason,
    };
    let conn = open_db(path)?;
    let has_table = |name: &str| -> Result<bool> {
        Ok(conn
            .query_row(
                "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1",
                [name],
                |_| Ok(()),
            )
            .optional()
            .map_err(|e| invalid(e.to_string()))?
            .is_some())
    };
    for table in ["Document", "Publication"] {
        if !has_table(table)? {
            return Err(invalid(format!("missing {table} table")));
        }
    }
    conn.query_row(
        "SELECT MepsLanguageIndex, Symbol, Year, IFNULL(CAST(IssueTagNumber AS TEXT), ''),
                IFNULL(Title, ''), ShortTitle, PublicationType
         FROM Publication LIMIT 1",
        [],
        |r| {
            Ok(PublicationInfo {
                meps_language: r.get(0)?,
                symbol: r.get(1)?,
                year: r.get(2)?,
                issue_tag: r.get(3)?,
                title: r.get(4)?,
                short_title: r.get(5)?,
                publication_type: r.get(6)?,
            })
        },
    )
    .optional()
    .map_err(|e| invalid(e.to_string()))?
    .ok_or_else(|| invalid("empty Publication table".into()))
}

struct HashingWriter<W, D> {
    inner: W,
    digest: D,
}

impl<W: Write, D: Digest> HashingWriter<W, D> {
    fn new(inner: W, digest: D) -> Self {
        Self { inner, digest }
    }

    fn finish(self) -> Vec<u8> {
        self.digest.finalize().to_vec()
    }
}

impl<W: Write, D: Digest> Write for HashingWriter<W, D> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let n = self.inner.write(buf)?;
        self.digest.update(&buf[..n]);
        Ok(n)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn safe_paths() {
        assert!(safe_relative_path("a.jpg").is_ok());
        assert!(safe_relative_path("dir/a.jpg").is_ok());
        for bad in [
            "",
            "../a",
            "a/../../b",
            "/etc/passwd",
            "a//b",
            "./a",
            "a\\..\\b",
            "C:/x",
            "a\0b",
        ] {
            assert!(safe_relative_path(bad).is_err(), "{bad:?} accepted");
        }
    }
}
