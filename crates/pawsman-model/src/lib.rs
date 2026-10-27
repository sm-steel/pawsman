//! The pawsman domain model: collections and requests, parsed from their files. No I/O.
#![forbid(unsafe_code)]

pub mod codes;
mod collection;
mod error;
pub mod format;
mod manifest;
mod request;
pub mod template;

pub use collection::{Collection, Entry, EntryKind};
pub use error::ModelError;
pub use manifest::Manifest;
pub use request::{Request, RequestId};
