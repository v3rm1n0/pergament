//! jwlinux: an unofficial reader library for `.jwpub` publications.
//!
//! See docs/FORMAT.md for the file format and where each detail comes from.

pub mod crypto;
mod error;
pub mod jwpub;
pub mod library;
pub mod manifest;
pub mod reader;
pub mod render;

pub use error::{Error, Result};
pub use jwpub::JwPub;
pub use library::{Entry, Library};
pub use reader::Publication;
pub use render::{RenderOptions, Renderer};
