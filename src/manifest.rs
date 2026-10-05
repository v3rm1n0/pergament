//! `manifest.json` of a `.jwpub` file. See docs/FORMAT.md.

use serde::Deserialize;

/// Upper bound for `manifest.json`; real ones are a few KiB.
pub const MAX_MANIFEST_SIZE: u64 = 1024 * 1024;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    pub name: String,
    /// SHA-256 (hex) of the raw `contents` entry.
    pub hash: String,
    #[serde(default)]
    pub timestamp: Option<String>,
    #[serde(default)]
    pub version: Option<u32>,
    #[serde(default)]
    pub expanded_size: Option<u64>,
    #[serde(default)]
    pub content_format: Option<String>,
    /// A float in some files and a string in others.
    #[serde(default)]
    pub meps_platform_version: Option<NumberOrString>,
    pub publication: ManifestPublication,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum NumberOrString {
    Number(f64),
    String(String),
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManifestPublication {
    /// Name of the SQLite database inside `contents`.
    pub file_name: String,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub short_title: Option<String>,
    pub symbol: String,
    /// MEPS language index.
    pub language: i64,
    #[serde(default)]
    pub year: Option<i64>,
    #[serde(default)]
    pub issue_id: Option<i64>,
    /// SHA-1 (hex) of the database file.
    #[serde(default)]
    pub hash: Option<String>,
    #[serde(default)]
    pub publication_type: Option<String>,
}

impl Manifest {
    pub fn parse(bytes: &[u8]) -> crate::Result<Self> {
        Ok(serde_json::from_slice(bytes)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BASE: &str = r#"{"name":"x_E.jwpub","hash":"00","publication":{"fileName":"x_E.db","symbol":"x","language":0}"#;

    #[test]
    fn platform_version_number_or_string() {
        let num =
            Manifest::parse(format!(r#"{BASE},"mepsPlatformVersion":2.1}}"#).as_bytes()).unwrap();
        assert!(matches!(
            num.meps_platform_version,
            Some(NumberOrString::Number(_))
        ));
        let s = Manifest::parse(format!(r#"{BASE},"mepsPlatformVersion":"2.10"}}"#).as_bytes())
            .unwrap();
        assert!(matches!(
            s.meps_platform_version,
            Some(NumberOrString::String(_))
        ));
    }

    #[test]
    fn missing_required_field_fails() {
        assert!(Manifest::parse(br#"{"name":"x","publication":{}}"#).is_err());
    }
}
