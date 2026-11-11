use std::ffi::{CStr, CString, c_char, c_int, c_void};
use std::ptr;
use std::sync::mpsc::{Sender, channel};
use std::time::Duration;

use pergament_ios::{
    pergament_backup, pergament_close, pergament_invoke, pergament_open, pergament_string_free,
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
    unsafe { pergament_string_free(err as *mut c_char) };
}
