//! Decoding of encrypted content columns. Scheme and sources in docs/FORMAT.md
//! ("Content encoding").

use std::io::Read;

use aes::Aes128;
use cbc::cipher::block_padding::Pkcs7;
use cbc::cipher::{BlockModeDecrypt, BlockModeEncrypt, KeyIvInit};
use flate2::Compression;
use flate2::read::ZlibDecoder;
use flate2::write::ZlibEncoder;
use sha2::{Digest, Sha256};

use crate::{Error, Result};

/// XOR constant from sws2apps/meeting-schedules-parser `getPubKeyIv`.
const XOR_KEY: [u8; 32] = [
    0x11, 0xcb, 0xb5, 0x58, 0x7e, 0x32, 0x84, 0x6d, 0x4c, 0x26, 0x79, 0x0c, 0x63, 0x3d, 0xa2, 0x89,
    0xf6, 0x6f, 0xe5, 0x84, 0x2a, 0x3a, 0x58, 0x5c, 0xe1, 0xbc, 0x3a, 0x29, 0x4a, 0xf5, 0xad, 0xa7,
];

/// Upper bound for one decoded blob. The largest seen is ~110 KiB.
const MAX_DECODED_SIZE: u64 = 64 * 1024 * 1024;

/// Key material for one publication.
#[derive(Clone)]
pub struct ContentKey {
    key: [u8; 16],
    iv: [u8; 16],
}

impl std::fmt::Debug for ContentKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ContentKey(..)")
    }
}

/// The string the key is derived from:
/// `{lang}_{symbol}_{year}` plus `_{issue}` unless the issue tag is 0 or empty.
pub fn publication_card(meps_language: i64, symbol: &str, year: i64, issue_tag: &str) -> String {
    let issue = issue_tag.trim();
    if issue.is_empty() || issue == "0" {
        format!("{meps_language}_{symbol}_{year}")
    } else {
        format!("{meps_language}_{symbol}_{year}_{issue}")
    }
}

impl ContentKey {
    pub fn from_card(card: &str) -> Self {
        let hash = Sha256::digest(card.as_bytes());
        let mut out = [0u8; 32];
        for (o, (h, x)) in out.iter_mut().zip(hash.iter().zip(XOR_KEY)) {
            *o = h ^ x;
        }
        let mut key = [0u8; 16];
        let mut iv = [0u8; 16];
        key.copy_from_slice(&out[..16]);
        iv.copy_from_slice(&out[16..]);
        Self { key, iv }
    }

    pub fn new(meps_language: i64, symbol: &str, year: i64, issue_tag: &str) -> Self {
        Self::from_card(&publication_card(meps_language, symbol, year, issue_tag))
    }

    /// Decrypt and inflate a content blob into bytes.
    pub fn decode_bytes(&self, blob: &[u8]) -> Result<Vec<u8>> {
        let compressed = cbc::Decryptor::<Aes128>::new(&self.key.into(), &self.iv.into())
            .decrypt_padded_vec::<Pkcs7>(blob)
            .map_err(|_| Error::Decode("bad ciphertext or wrong key".into()))?;
        let mut out = Vec::new();
        ZlibDecoder::new(compressed.as_slice())
            .take(MAX_DECODED_SIZE + 1)
            .read_to_end(&mut out)
            .map_err(|e| Error::Decode(format!("inflate: {e}")))?;
        if out.len() as u64 > MAX_DECODED_SIZE {
            return Err(Error::Decode("decoded content too large".into()));
        }
        Ok(out)
    }

    /// Decrypt and inflate a content blob into a UTF-8 string.
    pub fn decode(&self, blob: &[u8]) -> Result<String> {
        String::from_utf8(self.decode_bytes(blob)?)
            .map_err(|_| Error::Decode("content is not valid UTF-8".into()))
    }

    /// Inverse of [`ContentKey::decode`]; used to build test fixtures.
    pub fn encode(&self, plain: &[u8]) -> Vec<u8> {
        use std::io::Write;
        let mut z = ZlibEncoder::new(Vec::new(), Compression::default());
        z.write_all(plain).expect("writing to Vec cannot fail");
        let compressed = z.finish().expect("writing to Vec cannot fail");
        cbc::Encryptor::<Aes128>::new(&self.key.into(), &self.iv.into())
            .encrypt_padded_vec::<Pkcs7>(&compressed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn card_omits_zero_issue() {
        assert_eq!(publication_card(2, "nwtsty", 2025, "0"), "2_nwtsty_2025");
        assert_eq!(publication_card(2, "nwtsty", 2025, ""), "2_nwtsty_2025");
        assert_eq!(
            publication_card(2, "wp26", 2026, "20260900"),
            "2_wp26_2026_20260900"
        );
    }

    #[test]
    fn roundtrip() {
        let k = ContentKey::new(0, "test", 2020, "0");
        let html = "<p id=\"p1\">Hällo</p>".repeat(50);
        let blob = k.encode(html.as_bytes());
        assert_eq!(blob.len() % 16, 0);
        assert_eq!(k.decode(&blob).unwrap(), html);
    }

    #[test]
    fn wrong_key_fails() {
        let blob = ContentKey::new(0, "a", 2020, "0").encode(b"hello world");
        assert!(ContentKey::new(0, "b", 2020, "0").decode(&blob).is_err());
    }

    #[test]
    fn garbage_fails() {
        let k = ContentKey::new(0, "a", 2020, "0");
        assert!(k.decode(&[]).is_err());
        assert!(k.decode(&[1, 2, 3]).is_err());
        assert!(k.decode(&[0u8; 32]).is_err());
    }
}
