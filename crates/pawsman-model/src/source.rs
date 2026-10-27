//! The files a collection is built from, as the store read them.

use std::path::PathBuf;

/// One file of a collection: its path relative to the root (with `/` separators) and its bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceFile {
    path: PathBuf,
    bytes: Vec<u8>,
}

impl SourceFile {
    /// A file at `path`, relative to the collection root.
    #[must_use]
    pub fn new(path: PathBuf, bytes: Vec<u8>) -> Self {
        Self { path, bytes }
    }

    pub(crate) fn into_parts(self) -> (PathBuf, Vec<u8>) {
        (self.path, self.bytes)
    }
}
