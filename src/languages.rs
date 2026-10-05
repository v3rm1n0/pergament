//! The public jw.org language list (see docs/FORMAT.md, "Language list"),
//! cached locally. It maps language codes like `X` to names.

use std::fs;
use std::path::Path;
use std::time::{Duration, SystemTime};

use serde::{Deserialize, Serialize};

use crate::Result;
use crate::net::Client;

pub const LANGUAGES_URL: &str = "https://www.jw.org/en/languages/";
const CACHE_FILE: &str = "languages.json";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Language {
    /// Language code used by downloads and file names, e.g. `X`.
    pub code: String,
    /// English name, e.g. "German".
    pub name: String,
    /// Name in the language itself, e.g. "Deutsch".
    pub vernacular: String,
    pub rtl: bool,
    pub sign_language: bool,
}

#[derive(Deserialize)]
struct RawList {
    languages: Vec<RawLanguage>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawLanguage {
    langcode: String,
    name: String,
    #[serde(default)]
    vernacular_name: Option<String>,
    #[serde(default)]
    direction: Option<String>,
    #[serde(default)]
    is_sign_language: bool,
}

fn normalize(raw: RawList) -> Vec<Language> {
    raw.languages
        .into_iter()
        .filter(|l| {
            !l.langcode.is_empty()
                && l.langcode.len() <= 8
                && l.langcode
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-')
        })
        .map(|l| Language {
            vernacular: l.vernacular_name.unwrap_or_else(|| l.name.clone()),
            rtl: l.direction.as_deref() == Some("rtl"),
            code: l.langcode,
            name: l.name,
            sign_language: l.is_sign_language,
        })
        .collect()
}

/// Parse the jw.org response body.
pub fn parse(json: &[u8]) -> Result<Vec<Language>> {
    Ok(normalize(serde_json::from_slice(json)?))
}

/// Languages from the cache, refreshed from jw.org when older than `max_age`.
/// A stale cache is used if the download fails.
pub fn load(client: &Client, cache_dir: &Path, max_age: Duration) -> Result<Vec<Language>> {
    let path = cache_dir.join(CACHE_FILE);
    let cached =
        || -> Option<Vec<Language>> { serde_json::from_slice(&fs::read(&path).ok()?).ok() };
    let fresh = fs::metadata(&path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| SystemTime::now().duration_since(t).ok())
        .is_some_and(|age| age <= max_age);
    if fresh && let Some(list) = cached() {
        return Ok(list);
    }
    match client.get_json::<RawList>(LANGUAGES_URL) {
        Ok(raw) => {
            let list = normalize(raw);
            fs::create_dir_all(cache_dir)?;
            fs::write(&path, serde_json::to_vec(&list)?)?;
            Ok(list)
        }
        Err(e) => cached().ok_or(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_jw_org_list() {
        // Shape as returned on 2026-10-05 (shortened).
        let json = r#"{"status":200,"languages":[
            {"symbol":"de","langcode":"X","name":"German","vernacularName":"Deutsch","script":"ROMAN","altSpellings":["German","Deutsch"],"direction":"ltr","isSignLanguage":false,"isCounted":true,"hasWebContent":true},
            {"symbol":"ar","langcode":"A","name":"Arabic","vernacularName":"العربية","direction":"rtl","isSignLanguage":false},
            {"symbol":"ase","langcode":"ASL","name":"American Sign Language","vernacularName":"American Sign Language","direction":"ltr","isSignLanguage":true},
            {"symbol":"x","langcode":"../x","name":"Bad"}]}"#;
        let list = parse(json.as_bytes()).unwrap();
        assert_eq!(list.len(), 3);
        assert_eq!(list[0].code, "X");
        assert_eq!(list[0].vernacular, "Deutsch");
        assert!(list[1].rtl);
        assert!(list[2].sign_language);
    }
}
