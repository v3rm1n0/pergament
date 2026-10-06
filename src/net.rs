//! HTTP client for the public jw.org services: honest User-Agent, rate
//! limiting, retries with backoff and resumable, verified downloads.
//! Endpoints and their verification are documented in docs/FORMAT.md.

use std::cell::Cell;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant};

use md5::Md5;
use sha1::Sha1;
use sha2::Digest;
use ureq::ResponseExt;
use ureq::http::Response;

use crate::{Error, Result};

pub const USER_AGENT: &str = concat!(
    "pergament/",
    env!("CARGO_PKG_VERSION"),
    " (unofficial personal-use reader; no telemetry)"
);

/// Hosts Pergament talks to. Download URLs from API responses are untrusted and
/// must point at one of these (or a subdomain).
pub const DEFAULT_HOSTS: &[&str] = &["jw-cdn.org", "jw.org", "download-a.akamaihd.net"];

#[derive(Debug, Clone)]
pub struct HttpConfig {
    pub user_agent: String,
    /// Minimum time between the start of two requests.
    pub min_interval: Duration,
    /// Retries after the first attempt for network errors, 429 and 5xx.
    pub max_retries: u32,
    pub base_backoff: Duration,
    /// Cap for server-sent `Retry-After` and computed backoff.
    pub max_backoff: Duration,
    pub allowed_hosts: Vec<String>,
    /// Only for tests against a local server.
    pub allow_plain_http: bool,
}

impl Default for HttpConfig {
    fn default() -> Self {
        Self {
            user_agent: USER_AGENT.to_owned(),
            min_interval: Duration::from_secs(1),
            max_retries: 4,
            base_backoff: Duration::from_secs(1),
            max_backoff: Duration::from_secs(60),
            allowed_hosts: DEFAULT_HOSTS.iter().map(|s| (*s).to_owned()).collect(),
            allow_plain_http: false,
        }
    }
}

/// What a finished download must match. Every check that is present is
/// enforced.
#[derive(Debug, Clone, Default)]
pub struct Expected {
    pub size: Option<u64>,
    pub md5: Option<String>,
    pub sha1: Option<String>,
}

/// Upper bound when the size is not known in advance.
const MAX_UNKNOWN_SIZE: u64 = 4 << 30;
/// Upper bound for JSON API responses.
pub const MAX_JSON_SIZE: u64 = 16 << 20;

#[derive(Debug)]
pub struct Client {
    agent: ureq::Agent,
    config: HttpConfig,
    last_request: Cell<Option<Instant>>,
}

impl Client {
    pub fn new(config: HttpConfig) -> Self {
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .user_agent(config.user_agent.as_str())
            .https_only(!config.allow_plain_http)
            .http_status_as_error(false)
            .max_redirects(5)
            .timeout_connect(Some(Duration::from_secs(15)))
            .timeout_recv_response(Some(Duration::from_secs(30)))
            // A stalled body read fails instead of hanging forever.
            .timeout_recv_body(Some(Duration::from_secs(120)))
            .build()
            .into();
        Self {
            agent,
            config,
            last_request: Cell::new(None),
        }
    }

    pub fn config(&self) -> &HttpConfig {
        &self.config
    }

    /// Reject URLs that are not https (unless allowed) or not on an allowed host.
    pub fn check_url(&self, url: &str) -> Result<()> {
        let bad = |why: &str| Error::Http(format!("refusing URL {url:?}: {why}"));
        let rest = if let Some(r) = url.strip_prefix("https://") {
            r
        } else if self.config.allow_plain_http {
            url.strip_prefix("http://")
                .ok_or_else(|| bad("not http(s)"))?
        } else {
            return Err(bad("not https"));
        };
        let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
        if authority.contains('@') {
            return Err(bad("credentials in URL"));
        }
        let host = match authority.rsplit_once(':') {
            Some((h, port)) if port.chars().all(|c| c.is_ascii_digit()) => h,
            _ => authority,
        }
        .to_ascii_lowercase();
        let ok = self
            .config
            .allowed_hosts
            .iter()
            .any(|a| host == *a || host.ends_with(&format!(".{a}")));
        if ok {
            Ok(())
        } else {
            Err(bad("host not allowed"))
        }
    }

    fn throttle(&self) {
        if let Some(last) = self.last_request.get() {
            let elapsed = last.elapsed();
            if elapsed < self.config.min_interval {
                thread::sleep(self.config.min_interval - elapsed);
            }
        }
        self.last_request.set(Some(Instant::now()));
    }

    fn backoff(&self, attempt: u32, retry_after: Option<Duration>) -> Duration {
        let exp = self
            .config
            .base_backoff
            .saturating_mul(2u32.saturating_pow(attempt));
        retry_after.unwrap_or(exp).min(self.config.max_backoff)
    }

    /// One GET with retries on network errors, 429 and 5xx. Other statuses
    /// are returned to the caller.
    fn get(&self, url: &str, range_from: Option<u64>) -> Result<Response<ureq::Body>> {
        self.check_url(url)?;
        let mut attempt = 0;
        loop {
            self.throttle();
            let mut req = self.agent.get(url);
            if let Some(from) = range_from {
                req = req.header("Range", format!("bytes={from}-"));
            }
            let (retry_after, err) = match req.call() {
                Ok(resp) => {
                    self.check_url(&resp.get_uri().to_string())?;
                    let status = resp.status().as_u16();
                    if status == 429 || (500..600).contains(&status) {
                        (
                            retry_after(&resp),
                            Error::Http(format!("HTTP {status} for {url}")),
                        )
                    } else {
                        return Ok(resp);
                    }
                }
                Err(e) if is_transient(&e) => (None, Error::Http(format!("{e} for {url}"))),
                Err(e) => return Err(Error::Http(format!("{e} for {url}"))),
            };
            if attempt >= self.config.max_retries {
                return Err(err);
            }
            thread::sleep(self.backoff(attempt, retry_after));
            attempt += 1;
        }
    }

    /// GET a JSON document (bounded size). Non-2xx is an error.
    pub fn get_json<T: serde::de::DeserializeOwned>(&self, url: &str) -> Result<T> {
        let mut resp = self.get(url, None)?;
        let status = resp.status().as_u16();
        if !(200..300).contains(&status) {
            return Err(Error::Http(format!("HTTP {status} for {url}")));
        }
        let mut buf = Vec::new();
        resp.body_mut()
            .with_config()
            .limit(MAX_JSON_SIZE)
            .reader()
            .read_to_end(&mut buf)
            .map_err(|e| Error::Http(format!("reading {url}: {e}")))?;
        Ok(serde_json::from_slice(&buf)?)
    }

    /// Download `url` to `dest`, resuming from `dest.part` if present, then
    /// verify size and hashes and move the file into place.
    pub fn download(
        &self,
        url: &str,
        dest: &Path,
        expected: &Expected,
        progress: &mut dyn FnMut(u64, Option<u64>),
    ) -> Result<()> {
        let part = part_path(dest);
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent)?;
        }
        let limit = expected.size.unwrap_or(MAX_UNKNOWN_SIZE);
        let mut failures = 0;
        loop {
            match self.download_once(url, &part, limit, expected.size, progress) {
                Ok(()) => break,
                Err(DownloadError::Fatal(e)) => return Err(e),
                Err(DownloadError::Retry(e)) => {
                    if failures >= self.config.max_retries {
                        return Err(e);
                    }
                    thread::sleep(self.backoff(failures, None));
                    failures += 1;
                }
            }
        }
        if let Err(e) = verify_file(&part, expected) {
            // A corrupt partial file must not be resumed next time.
            let _ = fs::remove_file(&part);
            return Err(e);
        }
        fs::rename(&part, dest)?;
        Ok(())
    }

    fn download_once(
        &self,
        url: &str,
        part: &Path,
        limit: u64,
        total: Option<u64>,
        progress: &mut dyn FnMut(u64, Option<u64>),
    ) -> Result<(), DownloadError> {
        let have = fs::metadata(part).map(|m| m.len()).unwrap_or(0);
        if have > limit {
            fs::remove_file(part).map_err(|e| DownloadError::Fatal(e.into()))?;
            return Err(DownloadError::Retry(Error::Http(
                "partial file too large".into(),
            )));
        }
        if Some(have) == total && have > 0 {
            return Ok(());
        }
        let resp = self
            .get(url, (have > 0).then_some(have))
            .map_err(DownloadError::Fatal)?;
        let status = resp.status().as_u16();
        let (mut file, start) = match status {
            206 if content_range_start(&resp) == Some(have) => {
                let f = OpenOptions::new()
                    .append(true)
                    .open(part)
                    .map_err(|e| DownloadError::Fatal(e.into()))?;
                (f, have)
            }
            200 | 206 => {
                // Server ignored or mangled the range: start over.
                let f = File::create(part).map_err(|e| DownloadError::Fatal(e.into()))?;
                (f, 0)
            }
            416 if have > 0 => {
                // Range not satisfiable: the partial file is probably complete
                // already; verification decides.
                return Ok(());
            }
            _ => {
                return Err(DownloadError::Fatal(Error::Http(format!(
                    "HTTP {status} for {url}"
                ))));
            }
        };
        // Without an expected size, the advertised length gives progress and
        // detects truncated bodies; `limit` still bounds what is written.
        let total = total.or_else(|| {
            resp.headers()
                .get("content-length")?
                .to_str()
                .ok()?
                .parse::<u64>()
                .ok()
                .map(|n| start + n)
        });
        let mut done = start;
        progress(done, total);
        let mut reader = resp.into_body().into_reader();
        let mut buf = vec![0u8; 64 * 1024];
        loop {
            let n = match reader.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => n,
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                // Keep what we have; the next attempt resumes from here.
                Err(e) => return Err(DownloadError::Retry(Error::Http(format!("{e} for {url}")))),
            };
            done += n as u64;
            if done > limit {
                drop(file);
                let _ = fs::remove_file(part);
                return Err(DownloadError::Fatal(Error::TooLarge(format!(
                    "download exceeds {limit} bytes"
                ))));
            }
            file.write_all(&buf[..n])
                .map_err(|e| DownloadError::Fatal(e.into()))?;
            progress(done, total);
        }
        file.sync_all()
            .map_err(|e| DownloadError::Fatal(e.into()))?;
        match total {
            Some(t) if done < t => Err(DownloadError::Retry(Error::Http(format!(
                "connection closed after {done} of {t} bytes"
            )))),
            _ => Ok(()),
        }
    }
}

enum DownloadError {
    Retry(Error),
    Fatal(Error),
}

pub fn part_path(dest: &Path) -> PathBuf {
    let mut name = dest.file_name().unwrap_or_default().to_os_string();
    name.push(".part");
    dest.with_file_name(name)
}

fn is_transient(e: &ureq::Error) -> bool {
    matches!(
        e,
        ureq::Error::Io(_)
            | ureq::Error::Timeout(_)
            | ureq::Error::HostNotFound
            | ureq::Error::ConnectionFailed
            | ureq::Error::Protocol(_)
    )
}

fn retry_after(resp: &Response<ureq::Body>) -> Option<Duration> {
    resp.headers()
        .get("retry-after")?
        .to_str()
        .ok()?
        .trim()
        .parse()
        .ok()
        .map(Duration::from_secs)
}

/// Start offset from `Content-Range: bytes START-END/TOTAL`.
fn content_range_start(resp: &Response<ureq::Body>) -> Option<u64> {
    parse_content_range_start(resp.headers().get("content-range")?.to_str().ok()?)
}

pub fn parse_content_range_start(v: &str) -> Option<u64> {
    v.trim()
        .strip_prefix("bytes ")?
        .split('-')
        .next()?
        .trim()
        .parse()
        .ok()
}

/// Check size, MD5 and SHA-1 of a file against `expected`.
pub fn verify_file(path: &Path, expected: &Expected) -> Result<()> {
    let mut f = File::open(path)?;
    let mut md5 = Md5::new();
    let mut sha1 = Sha1::new();
    let mut size = 0u64;
    let mut buf = vec![0u8; 256 * 1024];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        md5.update(&buf[..n]);
        sha1.update(&buf[..n]);
        size += n as u64;
    }
    if let Some(want) = expected.size
        && want != size
    {
        return Err(Error::HashMismatch {
            what: "download size",
            expected: want.to_string(),
            actual: size.to_string(),
        });
    }
    let checks = [
        ("download MD5", &expected.md5, hex::encode(md5.finalize())),
        (
            "download SHA-1",
            &expected.sha1,
            hex::encode(sha1.finalize()),
        ),
    ];
    for (what, want, got) in checks {
        if let Some(want) = want
            && !want.trim().eq_ignore_ascii_case(&got)
        {
            return Err(Error::HashMismatch {
                what,
                expected: want.clone(),
                actual: got,
            });
        }
    }
    Ok(())
}

/// Percent-encode a query value.
pub fn encode_query(v: &str) -> String {
    let mut out = String::new();
    for b in v.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_checks() {
        let c = Client::new(HttpConfig::default());
        for ok in [
            "https://b.jw-cdn.org/apis/pub-media/GETPUBMEDIALINKS?x=1",
            "https://cfp2.jw-cdn.org/a/ebf1976/4/o/wp_X_202609.jwpub",
            "https://www.jw.org/en/languages/",
            "https://jw.org/",
        ] {
            assert!(c.check_url(ok).is_ok(), "{ok}");
        }
        for bad in [
            "http://b.jw-cdn.org/x",
            "https://evil.example/x",
            "https://jw-cdn.org.evil.example/x",
            "https://evil-jw-cdn.org/x",
            "https://user@b.jw-cdn.org/x",
            "https://b.jw-cdn.org@evil.example/x",
            "file:///etc/passwd",
            "ftp://jw.org/",
        ] {
            assert!(c.check_url(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn content_range() {
        assert_eq!(
            parse_content_range_start("bytes 100-199/2861525"),
            Some(100)
        );
        assert_eq!(parse_content_range_start("bytes */100"), None);
        assert_eq!(parse_content_range_start("items 1-2/3"), None);
    }

    #[test]
    fn query_encoding() {
        assert_eq!(encode_query("a b&c=ü"), "a%20b%26c%3D%C3%BC");
    }

    #[test]
    fn backoff_is_capped() {
        let c = Client::new(HttpConfig::default());
        assert_eq!(c.backoff(0, None), Duration::from_secs(1));
        assert_eq!(c.backoff(3, None), Duration::from_secs(8));
        assert_eq!(c.backoff(30, None), Duration::from_secs(60));
        assert_eq!(
            c.backoff(0, Some(Duration::from_secs(5))),
            Duration::from_secs(5)
        );
        assert_eq!(
            c.backoff(0, Some(Duration::from_secs(999))),
            Duration::from_secs(60)
        );
    }
}
