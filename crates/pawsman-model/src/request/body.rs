//! A request body: inline, in a sidecar file, a form, multipart parts or a file.

use std::path::{Path, PathBuf};

/// What `[body]` holds.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum Body {
    /// JSON text, kept as written.
    Json(Content),
    /// Plain text.
    Text(Content),
    /// `application/x-www-form-urlencoded` fields, in file order.
    Form(Vec<(String, String)>),
    /// `multipart/form-data` parts.
    Multipart(Vec<Part>),
    /// A file sent as is, relative to the request file.
    File(PathBuf),
}

/// The bytes of a JSON or text body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Content {
    /// UTF-8 text; templates apply.
    Utf8(String),
    /// Bytes that are not UTF-8, from a sidecar; sent as is.
    Binary(Vec<u8>),
}

/// One multipart part.
#[derive(Debug, Clone, PartialEq)]
pub struct Part {
    pub(crate) name: String,
    pub(crate) data: PartData,
    pub(crate) content_type: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PartData {
    Text(String),
    File(PathBuf),
}

/// What a part carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PartValue<'a> {
    /// A text value; templates apply.
    Text(&'a str),
    /// A file, relative to the request file.
    File(&'a Path),
}

impl Part {
    /// The field name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The value or file.
    #[must_use]
    pub fn value(&self) -> PartValue<'_> {
        match &self.data {
            PartData::Text(text) => PartValue::Text(text),
            PartData::File(path) => PartValue::File(path),
        }
    }

    /// The part's own `Content-Type`.
    #[must_use]
    pub fn content_type(&self) -> Option<&str> {
        self.content_type.as_deref()
    }
}

/// Reads the bytes of a sidecar file next to the request.
pub(crate) type Sidecars<'a> = &'a dyn Fn(&str) -> Option<Vec<u8>>;
