//! Download behaviour against a local HTTP server (no internet needed).

use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use md5::{Digest, Md5};
use pergament::Error;
use pergament::net::{Client, Expected, HttpConfig, part_path};

#[derive(Clone, Copy, PartialEq)]
enum Fault {
    None,
    /// First request answers 503 with Retry-After: 0.
    Unavailable,
    /// First request sends half the body, then closes the connection.
    CutOff,
    /// Ignore Range headers and always send 200 with the full body.
    NoRanges,
    /// Flip a byte in the body.
    Corrupt,
    InvalidRange,
    FreshPartial,
    FreshComplete,
    Unsatisfiable,
    RedirectDisallowed,
    RedirectRelative,
}

struct Server {
    url: String,
    requests: Arc<AtomicUsize>,
    ranges: Arc<Mutex<Vec<Option<u64>>>>,
}

fn serve(body: Vec<u8>, fault: Fault) -> Server {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!(
        "http://127.0.0.1:{}/file.jwpub",
        listener.local_addr().unwrap().port()
    );
    let requests = Arc::new(AtomicUsize::new(0));
    let ranges = Arc::new(Mutex::new(Vec::new()));
    let (req_count, seen) = (requests.clone(), ranges.clone());
    thread::spawn(move || {
        for stream in listener.incoming() {
            let mut stream = stream.unwrap();
            let n = req_count.fetch_add(1, Ordering::SeqCst);
            let mut range = None;
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            loop {
                let mut line = String::new();
                if reader.read_line(&mut line).unwrap() == 0 || line == "\r\n" {
                    break;
                }
                if let Some(v) = line.to_ascii_lowercase().strip_prefix("range: bytes=") {
                    range = v.trim().trim_end_matches('-').parse::<u64>().ok();
                }
            }
            seen.lock().unwrap().push(range);
            if fault == Fault::Unavailable && n == 0 {
                let _ = stream.write_all(
                    b"HTTP/1.1 503 Service Unavailable\r\nRetry-After: 0\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                );
                continue;
            }
            if fault == Fault::RedirectRelative && n == 0 {
                let _ = stream.write_all(b"HTTP/1.1 302 Found\r\nLocation: /target\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
                continue;
            }
            if fault == Fault::RedirectDisallowed && n == 0 {
                let port = stream.local_addr().unwrap().port();
                let response = format!("HTTP/1.1 302 Found\r\nLocation: http://localhost:{port}/target\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
                let _ = stream.write_all(response.as_bytes());
                continue;
            }
            if fault == Fault::Unsatisfiable && range.is_some() {
                let _ = stream.write_all(b"HTTP/1.1 416 Range Not Satisfiable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
                continue;
            }
            if fault == Fault::InvalidRange && range.is_some() {
                let _ = stream.write_all(b"HTTP/1.1 206 Partial Content\r\nContent-Range: bytes 5-7/10\r\nContent-Length: 3\r\nConnection: close\r\n\r\nBAD");
                continue;
            }
            if matches!(fault, Fault::FreshPartial | Fault::FreshComplete) && range.is_none() {
                let len = if fault == Fault::FreshPartial { body.len() / 2 } else { body.len() };
                let head = format!("HTTP/1.1 206 Partial Content\r\nContent-Range: bytes 0-{}/{}\r\nContent-Length: {len}\r\nConnection: close\r\n\r\n", len - 1, body.len());
                let _ = stream.write_all(head.as_bytes());
                let _ = stream.write_all(&body[..len]);
                continue;
            }
            let mut data = body.clone();
            if fault == Fault::Corrupt {
                data[0] ^= 0xff;
            }
            let start = if fault == Fault::NoRanges {
                None
            } else {
                range
            };
            let (status, slice, extra) = match start {
                Some(s) => (
                    "206 Partial Content",
                    &data[s as usize..],
                    format!(
                        "Content-Range: bytes {s}-{}/{}\r\n",
                        data.len() - 1,
                        data.len()
                    ),
                ),
                None => ("200 OK", &data[..], String::new()),
            };
            let head = format!(
                "HTTP/1.1 {status}\r\nContent-Length: {}\r\n{extra}Connection: close\r\n\r\n",
                slice.len()
            );
            let _ = stream.write_all(head.as_bytes());
            if fault == Fault::CutOff && n == 0 {
                let _ = stream.write_all(&slice[..slice.len() / 2]);
                let _ = stream.flush();
                continue; // drop the connection mid-body
            }
            let _ = stream.write_all(slice);
        }
    });
    Server {
        url,
        requests,
        ranges,
    }
}

fn client() -> Client {
    Client::new(HttpConfig {
        min_interval: Duration::ZERO,
        base_backoff: Duration::from_millis(10),
        max_backoff: Duration::from_millis(50),
        allowed_hosts: vec!["127.0.0.1".into()],
        allow_plain_http: true,
        ..HttpConfig::default()
    })
}

fn body() -> Vec<u8> {
    (0..300_000u32).map(|i| (i % 251) as u8).collect()
}

fn expected(data: &[u8]) -> Expected {
    Expected {
        size: Some(data.len() as u64),
        md5: Some(hex::encode(Md5::digest(data))),
        sha1: None,
    }
}

#[test]
fn downloads_and_verifies() {
    let data = body();
    let srv = serve(data.clone(), Fault::None);
    let tmp = tempfile::tempdir().unwrap();
    let dest = tmp.path().join("out.jwpub");
    let mut last = (0, None);
    client()
        .download(&srv.url, &dest, &expected(&data), &mut |d, t| last = (d, t))
        .unwrap();
    assert_eq!(std::fs::read(&dest).unwrap(), data);
    assert_eq!(last, (data.len() as u64, Some(data.len() as u64)));
    assert!(!part_path(&dest).exists());
}

#[test]
fn retries_after_503() {
    let data = body();
    let srv = serve(data.clone(), Fault::Unavailable);
    let tmp = tempfile::tempdir().unwrap();
    let dest = tmp.path().join("out.jwpub");
    client()
        .download(&srv.url, &dest, &expected(&data), &mut |_, _| {})
        .unwrap();
    assert_eq!(srv.requests.load(Ordering::SeqCst), 2);
    assert_eq!(std::fs::read(&dest).unwrap(), data);
}

#[test]
fn resumes_after_connection_loss() {
    let data = body();
    let srv = serve(data.clone(), Fault::CutOff);
    let tmp = tempfile::tempdir().unwrap();
    let dest = tmp.path().join("out.jwpub");
    client()
        .download(&srv.url, &dest, &expected(&data), &mut |_, _| {})
        .unwrap();
    assert_eq!(std::fs::read(&dest).unwrap(), data);
    let ranges = srv.ranges.lock().unwrap().clone();
    assert_eq!(ranges[0], None);
    assert!(
        matches!(ranges[1], Some(n) if n > 0),
        "second request must resume: {ranges:?}"
    );
}

#[test]
fn resumes_existing_part_file() {
    let data = body();
    let srv = serve(data.clone(), Fault::None);
    let tmp = tempfile::tempdir().unwrap();
    let dest = tmp.path().join("out.jwpub");
    std::fs::write(part_path(&dest), &data[..1000]).unwrap();
    client()
        .download(&srv.url, &dest, &expected(&data), &mut |_, _| {})
        .unwrap();
    assert_eq!(std::fs::read(&dest).unwrap(), data);
    assert_eq!(srv.ranges.lock().unwrap()[0], Some(1000));
}

#[test]
fn restarts_when_server_ignores_range() {
    let data = body();
    let srv = serve(data.clone(), Fault::NoRanges);
    let tmp = tempfile::tempdir().unwrap();
    let dest = tmp.path().join("out.jwpub");
    std::fs::write(part_path(&dest), &data[..1000]).unwrap();
    client()
        .download(&srv.url, &dest, &expected(&data), &mut |_, _| {})
        .unwrap();
    assert_eq!(std::fs::read(&dest).unwrap(), data);
}

#[test]
fn rejects_corrupt_download_and_discards_part() {
    let data = body();
    let srv = serve(data.clone(), Fault::Corrupt);
    let tmp = tempfile::tempdir().unwrap();
    let dest = tmp.path().join("out.jwpub");
    let err = client()
        .download(&srv.url, &dest, &expected(&data), &mut |_, _| {})
        .unwrap_err();
    assert!(
        matches!(
            err,
            Error::HashMismatch {
                what: "download MD5",
                ..
            }
        ),
        "{err}"
    );
    assert!(!dest.exists());
    assert!(!part_path(&dest).exists());
}

#[test]
fn rejects_oversize_download() {
    let data = body();
    let srv = serve(data.clone(), Fault::None);
    let tmp = tempfile::tempdir().unwrap();
    let dest = tmp.path().join("out.jwpub");
    let err = client()
        .download(
            &srv.url,
            &dest,
            &Expected {
                size: Some(1000),
                ..Expected::default()
            },
            &mut |_, _| {},
        )
        .unwrap_err();
    assert!(matches!(err, Error::TooLarge(_)), "{err}");
    assert!(!dest.exists());
}

#[test]
fn invalid_resumed_206_is_not_promoted() {
    let data = body();
    let srv = serve(data, Fault::InvalidRange);
    let tmp = tempfile::tempdir().unwrap();
    let dest = tmp.path().join("out");
    std::fs::write(part_path(&dest), b"old partial").unwrap();
    let expected = Expected { md5: Some("irrelevant".into()), ..Expected::default() };
    assert!(client().download(&srv.url, &dest, &expected, &mut |_, _| {}).is_err());
    assert!(!dest.exists());
    assert_eq!(std::fs::read(part_path(&dest)).unwrap(), b"old partial");
}

#[test]
fn unverified_existing_part_is_restarted() {
    let data = body();
    let srv = serve(data.clone(), Fault::None);
    let tmp = tempfile::tempdir().unwrap();
    let dest = tmp.path().join("out");
    std::fs::write(part_path(&dest), vec![0; data.len()]).unwrap();
    let expected = Expected { size: Some(data.len() as u64), ..Expected::default() };
    client().download(&srv.url, &dest, &expected, &mut |_, _| {}).unwrap();
    assert_eq!(std::fs::read(dest).unwrap(), data);
    assert_eq!(srv.ranges.lock().unwrap()[0], None);
}

#[test]
fn unverified_part_does_not_trust_416() {
    let data = body();
    let srv = serve(data.clone(), Fault::Unsatisfiable);
    let tmp = tempfile::tempdir().unwrap();
    let dest = tmp.path().join("out");
    std::fs::write(part_path(&dest), b"stale").unwrap();
    client().download(&srv.url, &dest, &Expected::default(), &mut |_, _| {}).unwrap();
    assert_eq!(std::fs::read(dest).unwrap(), data);
    assert_eq!(srv.ranges.lock().unwrap()[0], None);
}

#[test]
fn fresh_complete_206_is_accepted() {
    let data = body();
    let srv = serve(data.clone(), Fault::FreshComplete);
    let tmp = tempfile::tempdir().unwrap();
    let dest = tmp.path().join("out");
    client().download(&srv.url, &dest, &Expected::default(), &mut |_, _| {}).unwrap();
    assert_eq!(std::fs::read(dest).unwrap(), data);
}

#[test]
fn fresh_incomplete_206_is_not_promoted() {
    let data = body();
    let srv = serve(data, Fault::FreshPartial);
    let tmp = tempfile::tempdir().unwrap();
    let dest = tmp.path().join("out");
    assert!(client().download(&srv.url, &dest, &Expected::default(), &mut |_, _| {}).is_err());
    assert!(!dest.exists());
}

#[test]
fn follows_allowed_relative_redirect() {
    let data = body();
    let srv = serve(data.clone(), Fault::RedirectRelative);
    let tmp = tempfile::tempdir().unwrap();
    let dest = tmp.path().join("out");
    client().download(&srv.url, &dest, &Expected::default(), &mut |_, _| {}).unwrap();
    assert_eq!(std::fs::read(dest).unwrap(), data);
    assert_eq!(srv.requests.load(Ordering::SeqCst), 2);
}

#[test]
fn rejects_redirect_before_contacting_disallowed_host() {
    let srv = serve(vec![1], Fault::RedirectDisallowed);
    let tmp = tempfile::tempdir().unwrap();
    assert!(client().download(&srv.url, &tmp.path().join("out"), &Expected::default(), &mut |_, _| {}).is_err());
    assert_eq!(srv.requests.load(Ordering::SeqCst), 1);
}

#[test]
fn refuses_disallowed_hosts_without_request() {
    let c = Client::new(HttpConfig::default());
    let tmp = tempfile::tempdir().unwrap();
    let err = c
        .download(
            "https://evil.example/x.jwpub",
            &tmp.path().join("x"),
            &Expected::default(),
            &mut |_, _| {},
        )
        .unwrap_err();
    assert!(matches!(err, Error::Http(_)));
}

#[test]
fn rate_limit_spaces_requests() {
    let data = vec![1u8; 10];
    let srv = serve(data.clone(), Fault::None);
    let tmp = tempfile::tempdir().unwrap();
    let c = Client::new(HttpConfig {
        min_interval: Duration::from_millis(300),
        allowed_hosts: vec!["127.0.0.1".into()],
        allow_plain_http: true,
        ..HttpConfig::default()
    });
    let start = std::time::Instant::now();
    for i in 0..3 {
        c.download(
            &srv.url,
            &tmp.path().join(format!("{i}")),
            &Expected::default(),
            &mut |_, _| {},
        )
        .unwrap();
    }
    assert!(start.elapsed() >= Duration::from_millis(600));
}
