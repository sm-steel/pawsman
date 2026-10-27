//! The pawsman domain model: collections and requests, parsed from their files. No I/O.
#![forbid(unsafe_code)]

pub mod codes;
mod collection;
pub mod dotenv;
mod environment;
mod error;
mod folder;
pub mod format;
mod manifest;
mod request;
pub mod secrets;
mod source;
pub mod template;
pub mod write;

pub use collection::{Collection, Entry, EntryKind};
pub use environment::Environment;
pub use error::ModelError;
pub use folder::{Folder, Node};
pub use manifest::Manifest;
pub use request::{Body, Content, Part, PartValue, Request, RequestId, Script};
pub use source::SourceFile;
