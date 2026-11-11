use std::ffi::{CStr, CString, c_char, c_int, c_void};
use std::ptr;
use std::sync::mpsc::{Sender, channel};
use std::time::Duration;

use pergament_ios::{
    pergament_backup, pergament_close, pergament_invoke, pergament_media_free,
    pergament_media_headers, pergament_media_open, pergament_media_read, pergament_media_scheme,
    pergament_media_status, pergament_open, pergament_string_free,
};

type Reply = (u64, c_int, String);

unsafe extern "C" fn on_reply(ctx: *mut c_void, id: u64, ok: c_int, json: *const c_char) {
    // SAFETY: ctx is the leaked Sender<Reply> passed to pergament_open; json is NUL-terminated.
    let tx = unsafe { &*(ctx as *const Sender<Reply>) };
    let json = unsafe { CStr::from_ptr(json) }.to_str().unwrap().to_owned();
    tx.send((id, ok, json)).unwrap();
}

unsafe extern "C" fn on_event(_: *mut c_void, _: *const c_char, _: *const c_char) {}

fn cstr(s: &str) -> CString {
    CString::new(s).unwrap()
}

struct Fixture {
    handle: *mut c_void,
    rx: std::sync::mpsc::Receiver<Reply>,
    library: std::path::PathBuf,
    _dir: tempfile::TempDir,
}

fn open() -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let lib = cstr(dir.path().join("lib").to_str().unwrap());
    let cache = cstr(dir.path().join("cache").to_str().unwrap());
    let (tx, rx) = channel::<Reply>();
    let ctx = Box::into_raw(Box::new(tx)) as *mut c_void;
    let mut err: *const c_char = ptr::null();
    // SAFETY: the C strings outlive the call; ctx is leaked so it outlives every callback.
    let handle = unsafe {
        pergament_open(
            lib.as_ptr(),
            cache.as_ptr(),
            ctx,
            on_reply,
            on_event,
            &mut err,
        )
    };
    assert!(!handle.is_null());
    assert!(err.is_null());
    Fixture {
        handle,
        rx,
        library: dir.path().join("lib"),
        _dir: dir,
    }
}

fn call(f: &Fixture, id: u64, name: &str, args: &str) -> Reply {
    let (name, args) = (cstr(name), cstr(args));
    // SAFETY: handle is live and the strings outlive the call.
    unsafe { pergament_invoke(f.handle, id, name.as_ptr(), args.as_ptr()) };
    f.rx.recv_timeout(Duration::from_secs(10))
        .expect("no reply within 10 s")
}

#[test]
fn lists_an_empty_library() {
    let f = open();
    assert_eq!(
        call(&f, 7, "list_publications", "{}"),
        (7, 1, "[]".to_owned())
    );
    unsafe { pergament_backup(f.handle) };
    unsafe { pergament_close(f.handle) };
}

#[test]
fn unknown_command_fails() {
    let f = open();
    let (id, ok, json) = call(&f, 1, "no_such_command", "null");
    assert_eq!((id, ok), (1, 0));
    assert_eq!(json, "\"unknown command: no_such_command\"");
    unsafe { pergament_close(f.handle) };
}

#[test]
fn non_ascii_survives_the_round_trip() {
    let f = open();
    let (_, ok, json) = call(&f, 2, "ünbekannt", "");
    assert_eq!(ok, 0);
    let text: String = serde_json::from_str(&json).unwrap();
    assert_eq!(text, "unknown command: ünbekannt");
    unsafe { pergament_close(f.handle) };
}

#[test]
fn invalid_args_reply_with_an_error() {
    let f = open();
    let (_, ok, json) = call(&f, 3, "list_publications", "{nope");
    assert_eq!(ok, 0);
    assert!(
        serde_json::from_str::<String>(&json)
            .unwrap()
            .starts_with("list_publications: ")
    );
    unsafe { pergament_close(f.handle) };
}

#[test]
fn open_failure_reports_the_error() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("file");
    std::fs::write(&file, b"x").unwrap();
    let lib = cstr(file.join("lib").to_str().unwrap());
    let cache = cstr(dir.path().join("cache").to_str().unwrap());
    let (tx, _rx) = channel::<Reply>();
    let ctx = Box::into_raw(Box::new(tx)) as *mut c_void;
    let mut err: *const c_char = ptr::null();
    // SAFETY: as in `open`.
    let handle = unsafe {
        pergament_open(
            lib.as_ptr(),
            cache.as_ptr(),
            ctx,
            on_reply,
            on_event,
            &mut err,
        )
    };
    assert!(handle.is_null());
    assert!(!err.is_null());
    let msg = unsafe { CStr::from_ptr(err) }.to_str().unwrap();
    assert!(!msg.is_empty());
    unsafe { pergament_string_free(err) };
}

#[test]
fn null_name_replies_with_an_error() {
    let f = open();
    // SAFETY: handle is live; a NULL name is allowed.
    unsafe { pergament_invoke(f.handle, 9, ptr::null(), ptr::null()) };
    let (id, ok, _) =
        f.rx.recv_timeout(Duration::from_secs(10))
            .expect("no reply within 10 s");
    assert_eq!((id, ok), (9, 0));
    assert!(f.rx.recv_timeout(Duration::from_millis(200)).is_err());
    unsafe { pergament_close(f.handle) };
}

/// `/` + encodeURIComponent(path), as the shim's convertFileSrc builds it.
fn asset_path(path: &std::path::Path) -> CString {
    let mut out = String::from("/");
    for b in path.to_str().unwrap().bytes() {
        if b.is_ascii_alphanumeric() || b"-_.!~*'()".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    cstr(&out)
}

#[test]
fn media_scheme_is_the_picture_scheme() {
    let scheme = unsafe { CStr::from_ptr(pergament_media_scheme()) };
    assert_eq!(scheme.to_str().unwrap(), "jwmedia");
}

#[test]
fn media_serves_a_byte_range_and_outlives_the_handle() {
    let f = open();
    let media_dir = pergament::Library::open(&f.library).unwrap().media_dir();
    let file = media_dir.join("a b.mp4");
    std::fs::write(&file, (0u8..10).collect::<Vec<_>>()).unwrap();
    let (scheme, path, range) = (cstr("asset"), asset_path(&file), cstr("bytes=2-4"));
    // SAFETY: handle is live and the strings outlive the call.
    let media =
        unsafe { pergament_media_open(f.handle, scheme.as_ptr(), path.as_ptr(), range.as_ptr()) };
    assert!(!media.is_null());
    unsafe { pergament_close(f.handle) };

    assert_eq!(unsafe { pergament_media_status(media) }, 206);
    let headers = unsafe { CStr::from_ptr(pergament_media_headers(media)) };
    let headers: serde_json::Value = serde_json::from_str(headers.to_str().unwrap()).unwrap();
    assert_eq!(headers["Content-Range"], "bytes 2-4/10");
    assert_eq!(headers["Content-Length"], "3");
    assert_eq!(headers["Content-Type"], "video/mp4");

    let mut body = Vec::new();
    let mut buf = [0u8; 2];
    loop {
        let n = unsafe { pergament_media_read(media, buf.as_mut_ptr(), buf.len()) };
        assert!(n >= 0);
        if n == 0 {
            break;
        }
        body.extend_from_slice(&buf[..n as usize]);
    }
    assert_eq!(body, [2, 3, 4]);
    unsafe { pergament_media_free(media) };
}

#[test]
fn a_missing_picture_is_an_empty_404() {
    let f = open();
    let (scheme, path) = (cstr("jwmedia"), cstr("/nope/img_1.jpg"));
    let media =
        unsafe { pergament_media_open(f.handle, scheme.as_ptr(), path.as_ptr(), ptr::null()) };
    assert!(!media.is_null());
    assert_eq!(unsafe { pergament_media_status(media) }, 404);
    let headers = unsafe { CStr::from_ptr(pergament_media_headers(media)) };
    assert_eq!(headers.to_str().unwrap(), "{}");
    let mut buf = [0u8; 4];
    assert_eq!(
        unsafe { pergament_media_read(media, buf.as_mut_ptr(), buf.len()) },
        0
    );
    unsafe { pergament_media_free(media) };
    unsafe { pergament_close(f.handle) };
}

#[test]
fn media_open_rejects_an_unknown_scheme_and_null_arguments() {
    let f = open();
    let (other, asset, path) = (cstr("https"), cstr("asset"), cstr("/x"));
    unsafe {
        assert!(
            pergament_media_open(f.handle, other.as_ptr(), path.as_ptr(), ptr::null()).is_null()
        );
        assert!(pergament_media_open(f.handle, asset.as_ptr(), ptr::null(), ptr::null()).is_null());
        assert!(pergament_media_open(f.handle, ptr::null(), path.as_ptr(), ptr::null()).is_null());
        assert!(
            pergament_media_open(ptr::null_mut(), asset.as_ptr(), path.as_ptr(), ptr::null())
                .is_null()
        );
        pergament_media_free(ptr::null_mut());
        pergament_close(f.handle);
    }
}
