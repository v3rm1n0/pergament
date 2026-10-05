//! jwlinux: an unofficial reader library for `.jwpub` publications.
//!
//! See docs/FORMAT.md for the file format and where each detail comes from.

mod error;
pub mod manifest;

pub use error::{Error, Result};
