//! Tauri-free app logic shared by the desktop shell and the iOS core.

pub mod api;
pub mod auto;
pub mod core;
mod dispatch;
pub mod media;

pub use crate::core::{Core, Host, Paths};
