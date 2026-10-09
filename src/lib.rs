//! Pergament: an unofficial reader library for `.jwpub` publications.
//!
//! See docs/FORMAT.md for the file format and where each detail comes from.

pub mod catalog;
pub mod crypto;
mod error;
pub mod jwpub;
pub mod languages;
pub mod library;
pub mod links;
pub mod manifest;
pub mod mediator;
pub mod navigate;
pub mod net;
pub mod reader;
pub mod remote;
pub mod render;
pub mod userdata;
pub mod yeartext;

pub use error::{Error, Result};
pub use jwpub::JwPub;
pub use library::{Entry, Library};
pub use reader::Publication;
pub use render::{RenderOptions, Renderer};
