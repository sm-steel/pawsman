//! The pawsman domain model: collections and requests, parsed from their files. No I/O.
#![forbid(unsafe_code)]

pub mod codes;
mod collection;
pub mod dotenv;
mod environment;
mod error;
mod folder;
pub mod format;
mod ids;
mod interpolate;
mod manifest;
pub mod redact;
mod request;
pub mod scope;
pub mod secrets;
mod source;
pub mod template;
pub mod trace;
pub mod write;

pub use collection::{Collection, Entry, EntryKind};
pub use environment::Environment;
pub use error::ModelError;
pub use folder::{Folder, Node};
pub use ids::Ids;
pub use interpolate::{
    ResolvedBody, ResolvedPart, ResolvedPartValue, ResolvedRequest, interpolate,
};
pub use manifest::Manifest;
pub use redact::Redactor;
pub use request::{Body, Content, Part, PartValue, Request, RequestId, Script};
pub use source::SourceFile;
