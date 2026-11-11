//! Tauri-free app logic shared by the desktop shell and the iOS core.

pub mod api;
pub mod auto;
pub mod core;

pub use crate::core::{Core, Host, Paths};
