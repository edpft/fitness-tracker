//! Reading an authored programme from a document.
//!
//! The whole of this feature's TOML surface, kept at the adapter where § 21
//! permits an interface language to live.

pub mod document;
pub mod draft;

pub use document::{Document, DocumentError};
