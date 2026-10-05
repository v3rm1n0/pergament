use std::io;
use std::path::PathBuf;

/// Errors returned by the jwlinux library.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),

    #[error("invalid zip archive: {0}")]
    Zip(#[from] zip::result::ZipError),

    #[error("invalid manifest.json: {0}")]
    Manifest(#[from] serde_json::Error),

    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),

    #[error("archive is missing required entry `{0}`")]
    MissingEntry(String),

    #[error("{what} hash mismatch: expected {expected}, got {actual}")]
    HashMismatch {
        what: &'static str,
        expected: String,
        actual: String,
    },

    #[error("unsafe path in archive: {0:?}")]
    UnsafePath(String),

    #[error("archive exceeds limit: {0}")]
    TooLarge(String),

    #[error("invalid publication database {path}: {reason}")]
    InvalidDatabase { path: PathBuf, reason: String },

    #[error("cannot decode content: {0}")]
    Decode(String),

    #[error("not found: {0}")]
    NotFound(String),

    #[error("no data directory available (set XDG_DATA_HOME or HOME)")]
    NoDataDir,
}

pub type Result<T, E = Error> = std::result::Result<T, E>;
