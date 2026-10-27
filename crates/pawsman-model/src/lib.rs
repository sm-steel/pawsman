//! The pawsman domain model: collections and requests, parsed from their files. No I/O.
#![forbid(unsafe_code)]

pub mod codes;
mod collection;
mod error;
mod request;
pub mod template;

pub use collection::{Collection, Entry, EntryKind};
pub use error::ModelError;
pub use request::{Request, RequestId};
