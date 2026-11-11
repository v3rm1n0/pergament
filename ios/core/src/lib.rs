//! C bridge from the Pergament core to the iOS shell.

use std::ffi::{CStr, c_char};

// Pull the core into the archive so the cross-compile covers it.
use pergament as _;

const VERSION: &CStr =
    match CStr::from_bytes_with_nul(concat!(env!("CARGO_PKG_VERSION"), "\0").as_bytes()) {
        Ok(version) => version,
        Err(_) => panic!("the package version contains a NUL byte"),
    };

/// The core version as a NUL-terminated UTF-8 string. The pointer is
/// `'static`; the caller must not free it.
#[unsafe(no_mangle)]
pub extern "C" fn pergament_ios_version() -> *const c_char {
    VERSION.as_ptr()
}
