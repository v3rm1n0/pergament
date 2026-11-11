//! C bridge from the Pergament core to the iOS shell.

use std::ffi::{CStr, CString, c_char, c_int, c_void};
use std::io::Read;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::PathBuf;
use std::sync::{Arc, LazyLock};

use pergament_app_core::api;
use pergament_app_core::media::{self, MediaBody, MediaRoute};
use pergament_app_core::{Core, Host, Paths};

const VERSION: &CStr =
    match CStr::from_bytes_with_nul(concat!(env!("CARGO_PKG_VERSION"), "\0").as_bytes()) {
        Ok(version) => version,
        Err(_) => panic!("the package version contains a NUL byte"),
    };

type ReplyFn = unsafe extern "C" fn(ctx: *mut c_void, id: u64, ok: c_int, json: *const c_char);
type EventFn = unsafe extern "C" fn(ctx: *mut c_void, event: *const c_char, json: *const c_char);

/// The shell's opaque context pointer. Rust only hands it back, never dereferences it.
#[derive(Clone, Copy)]
struct Ctx(*mut c_void);

// SAFETY: the pointer is never dereferenced here; the header requires the shell's context to be usable from any thread.
unsafe impl Send for Ctx {}
unsafe impl Sync for Ctx {}

#[derive(Clone, Copy)]
struct Callbacks {
    ctx: Ctx,
    reply: ReplyFn,
    event: EventFn,
}

impl Callbacks {
    fn reply(&self, id: u64, ok: bool, json: &str) {
        // Serialized JSON has no NUL byte, so this does not fail.
        let Ok(json) = CString::new(json) else { return };
        // SAFETY: the shell keeps ctx and the callbacks valid until every id has replied.
        unsafe { (self.reply)(self.ctx.0, id, c_int::from(ok), json.as_ptr()) };
    }

    fn reply_err(&self, id: u64, message: &str) {
        let json = serde_json::to_string(message).unwrap_or_else(|_| "\"error\"".to_owned());
        self.reply(id, false, &json);
    }
}

impl Host for Callbacks {
    fn emit(&self, event: &str, payload: serde_json::Value) {
        let (Ok(name), Ok(payload)) = (CString::new(event), CString::new(payload.to_string()))
        else {
            return;
        };
        // SAFETY: as in `reply`; both strings outlive the call.
        unsafe { (self.event)(self.ctx.0, name.as_ptr(), payload.as_ptr()) };
    }
}

struct Handle {
    core: Arc<Core>,
    callbacks: Callbacks,
}

/// # Safety
/// `p` must be NULL or a NUL-terminated string.
unsafe fn read_str<'a>(p: *const c_char) -> Option<&'a str> {
    if p.is_null() {
        return None;
    }
    // SAFETY: guaranteed by the caller.
    unsafe { CStr::from_ptr(p) }.to_str().ok()
}

fn set_error(error_out: *mut *const c_char, message: &str) {
    if error_out.is_null() {
        return;
    }
    let message = CString::new(message.replace('\0', " ")).unwrap_or_default();
    // SAFETY: the caller passed a writable pointer; the string is released by `pergament_string_free`.
    unsafe { *error_out = message.into_raw() };
}

/// The core version as a NUL-terminated UTF-8 string. The pointer is
/// `'static`; the caller must not free it.
#[unsafe(no_mangle)]
pub extern "C" fn pergament_ios_version() -> *const c_char {
    VERSION.as_ptr()
}

/// Open the core. Returns NULL on failure and, when `error_out` is non-NULL,
/// stores a message there that the caller frees with `pergament_string_free`.
///
/// # Safety
/// The directory arguments must be NUL-terminated strings or NULL, and `error_out` NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pergament_open(
    library_dir: *const c_char,
    cache_dir: *const c_char,
    ctx: *mut c_void,
    reply: ReplyFn,
    event: EventFn,
    error_out: *mut *const c_char,
) -> *mut c_void {
    if !error_out.is_null() {
        // SAFETY: checked non-NULL, writable per the contract.
        unsafe { *error_out = std::ptr::null() };
    }
    // SAFETY: per this function's contract.
    let (Some(library), Some(cache)) = (unsafe { read_str(library_dir) }, unsafe {
        read_str(cache_dir)
    }) else {
        set_error(
            error_out,
            "library and cache directories must be UTF-8 strings",
        );
        return std::ptr::null_mut();
    };
    let callbacks = Callbacks {
        ctx: Ctx(ctx),
        reply,
        event,
    };
    let paths = Paths {
        library: PathBuf::from(library),
        cache: PathBuf::from(cache),
    };
    match Core::open(paths, Arc::new(callbacks)) {
        Ok(core) => Box::into_raw(Box::new(Handle {
            core: Arc::new(core),
            callbacks,
        }))
        .cast(),
        Err(message) => {
            set_error(error_out, &message);
            std::ptr::null_mut()
        }
    }
}

/// Run a command on a worker thread; `reply` is called exactly once. It normally arrives on the
/// worker thread, but is called on the calling thread, before this returns, if the worker cannot
/// be started. The caller must therefore register `id` before calling and must not hold a lock
/// the callback takes.
///
/// # Safety
/// `handle` must be NULL or from `pergament_open` and not yet closed; the strings NULL or NUL-terminated.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pergament_invoke(
    handle: *mut c_void,
    id: u64,
    name: *const c_char,
    args_json: *const c_char,
) {
    if handle.is_null() {
        return;
    }
    // SAFETY: a live handle per the contract.
    let handle = unsafe { &*handle.cast::<Handle>() };
    let callbacks = handle.callbacks;
    // SAFETY: per the contract.
    let (name, args) = unsafe { (read_str(name), read_str(args_json)) };
    let name = name.map(str::to_owned);
    let args = match args {
        Some(text) if !text.trim().is_empty() => text.to_owned(),
        _ => "null".to_owned(),
    };
    let core = Arc::clone(&handle.core);
    let spawned = std::thread::Builder::new()
        .name("pergament-invoke".to_owned())
        .spawn(move || {
            let Some(name) = name else {
                return callbacks.reply_err(id, "the command name is missing or not UTF-8");
            };
            let args = match serde_json::from_str(&args) {
                Ok(args) => args,
                Err(e) => return callbacks.reply_err(id, &format!("{name}: {e}")),
            };
            match catch_unwind(AssertUnwindSafe(|| core.invoke(&name, args))) {
                Ok(Ok(value)) => callbacks.reply(id, true, &value.to_string()),
                Ok(Err(message)) => callbacks.reply_err(id, &message),
                Err(_) => callbacks.reply_err(id, &format!("{name}: internal error")),
            }
        });
    if let Err(e) = spawned {
        callbacks.reply_err(id, &format!("could not start a worker: {e}"));
    }
}

/// Run the close-time backup on the calling thread.
///
/// # Safety
/// `handle` must be NULL or from `pergament_open` and not yet closed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pergament_backup(handle: *mut c_void) {
    if handle.is_null() {
        return;
    }
    // SAFETY: a live handle per the contract.
    unsafe { &*handle.cast::<Handle>() }.core.backup_on_close();
}

/// Back up and release the handle. Does not wait for calls in flight.
///
/// # Safety
/// `handle` must be NULL or from `pergament_open`, and is invalid afterwards.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pergament_close(handle: *mut c_void) {
    if handle.is_null() {
        return;
    }
    // SAFETY: the handle came from `Box::into_raw` and the caller gives up ownership.
    let handle = unsafe { Box::from_raw(handle.cast::<Handle>()) };
    handle.core.backup_on_close();
}

static MEDIA_SCHEME: LazyLock<CString> =
    LazyLock::new(|| CString::new(api::MEDIA_SCHEME).expect("the media scheme has no NUL byte"));

/// The scheme of picture URLs. The pointer is `'static`; the caller must not free it.
#[unsafe(no_mangle)]
pub extern "C" fn pergament_media_scheme() -> *const c_char {
    MEDIA_SCHEME.as_ptr()
}

/// An opened media response. Owns its file, so it does not depend on the handle staying open.
struct Media {
    status: c_int,
    headers: CString,
    body: MediaBody,
}

/// Open the response for a picture (`jwmedia`) or media folder file (`asset`) URL path. Blocks,
/// possibly on the network. Returns NULL for an unknown scheme, a NULL or non-UTF-8 argument, or
/// an internal error; otherwise a response released by `pergament_media_free`.
///
/// # Safety
/// `handle` must be NULL or from `pergament_open` and not yet closed; the strings NULL or NUL-terminated.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pergament_media_open(
    handle: *mut c_void,
    scheme: *const c_char,
    path: *const c_char,
    range: *const c_char,
) -> *mut c_void {
    if handle.is_null() {
        return std::ptr::null_mut();
    }
    // SAFETY: per the contract.
    let (scheme, path) = unsafe { (read_str(scheme), read_str(path)) };
    let (Some(scheme), Some(path)) = (scheme, path) else {
        return std::ptr::null_mut();
    };
    let route = match scheme {
        "asset" => MediaRoute::File,
        s if s == api::MEDIA_SCHEME => MediaRoute::Picture,
        _ => return std::ptr::null_mut(),
    };
    // SAFETY: per the contract.
    let range = unsafe { read_str(range) };
    // SAFETY: a live handle per the contract; the Arc keeps the core alive past a close.
    let core = Arc::clone(&unsafe { &*handle.cast::<Handle>() }.core);
    let opened = catch_unwind(AssertUnwindSafe(|| {
        let response = media::media_response(&core, route, path, range);
        let headers: serde_json::Map<String, serde_json::Value> = response
            .headers
            .into_iter()
            .map(|(name, value)| (name, value.into()))
            .collect();
        Media {
            status: c_int::from(response.status),
            headers: CString::new(serde_json::Value::Object(headers).to_string())
                .unwrap_or_default(),
            body: response.body,
        }
    }));
    match opened {
        Ok(media) => Box::into_raw(Box::new(media)).cast(),
        Err(_) => std::ptr::null_mut(),
    }
}

/// The HTTP status of a response; 0 for NULL.
///
/// # Safety
/// `media` must be NULL or from `pergament_media_open` and not yet freed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pergament_media_status(media: *const c_void) -> c_int {
    if media.is_null() {
        return 0;
    }
    // SAFETY: a live response per the contract.
    unsafe { &*media.cast::<Media>() }.status
}

/// The response headers as a JSON object of strings, valid until `pergament_media_free`.
/// NULL for a NULL response.
///
/// # Safety
/// `media` must be NULL or from `pergament_media_open` and not yet freed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pergament_media_headers(media: *const c_void) -> *const c_char {
    if media.is_null() {
        return std::ptr::null();
    }
    // SAFETY: a live response per the contract.
    unsafe { &*media.cast::<Media>() }.headers.as_ptr()
}

/// Read up to `cap` body bytes into `buf`. Returns the count, 0 at the end, -1 on a read error
/// or a NULL argument.
///
/// # Safety
/// `media` must be NULL or from `pergament_media_open` and not yet freed, used by one thread at a
/// time; `buf` must be NULL or writable for `cap` bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pergament_media_read(
    media: *mut c_void,
    buf: *mut u8,
    cap: usize,
) -> isize {
    if media.is_null() || buf.is_null() {
        return -1;
    }
    // SAFETY: a live response used by this thread alone, and a writable buffer, per the contract.
    let (media, buf) = unsafe {
        (
            &mut *media.cast::<Media>(),
            std::slice::from_raw_parts_mut(buf, cap.min(isize::MAX as usize)),
        )
    };
    let read = catch_unwind(AssertUnwindSafe(|| {
        loop {
            match media.body.read(buf) {
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                other => return other,
            }
        }
    }));
    match read {
        Ok(Ok(n)) => n as isize,
        _ => -1,
    }
}

/// Release a response. NULL is ignored.
///
/// # Safety
/// `media` must be NULL or from `pergament_media_open`, freed at most once.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pergament_media_free(media: *mut c_void) {
    if !media.is_null() {
        // SAFETY: the response came from `Box::into_raw` and the caller gives up ownership.
        drop(unsafe { Box::from_raw(media.cast::<Media>()) });
    }
}

/// Free a string returned through `error_out`. NULL is ignored.
///
/// # Safety
/// `s` must be NULL or a string produced by this library, freed at most once.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pergament_string_free(s: *const c_char) {
    if !s.is_null() {
        // SAFETY: allocated by `CString::into_raw` in `set_error`.
        drop(unsafe { CString::from_raw(s.cast_mut()) });
    }
}
