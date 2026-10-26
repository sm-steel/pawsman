//! The model crate's error.

use std::borrow::Cow;
use std::fmt;
use std::path::{Path, PathBuf};

use pawsman_error::{ErrorCode, ErrorInfo};

use crate::codes;

/// An error from `pawsman-model`.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct ModelError {
    kind: Kind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Kind {
    NotACollection { dir: PathBuf },
    InvalidFile { path: PathBuf, reason: String },
}

impl ModelError {
    /// `dir` has no `pawsman.toml`.
    #[must_use]
    pub fn not_a_collection(dir: &Path) -> Self {
        Self {
            kind: Kind::NotACollection {
                dir: dir.to_owned(),
            },
        }
    }

    /// `path` is not a valid file, for `reason` (one line).
    #[must_use]
    pub fn invalid_file(path: &Path, reason: &str) -> Self {
        Self {
            kind: Kind::InvalidFile {
                path: path.to_owned(),
                reason: reason.replace('\n', " ").trim().to_owned(),
            },
        }
    }

    /// `path` cannot be read.
    #[must_use]
    pub fn unreadable(path: &Path, err: &std::io::Error) -> Self {
        Self::invalid_file(path, &err.to_string())
    }
}

impl fmt::Display for ModelError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.kind {
            Kind::NotACollection { dir } => write!(f, "no `pawsman.toml` in `{}`", dir.display()),
            Kind::InvalidFile { path, reason } => {
                write!(f, "`{}` is not valid: {reason}", path.display())
            }
        }
    }
}

impl std::error::Error for ModelError {}

impl ErrorInfo for ModelError {
    fn code(&self) -> ErrorCode {
        match self.kind {
            Kind::NotACollection { .. } => codes::NOT_A_COLLECTION,
            Kind::InvalidFile { .. } => codes::INVALID_FILE,
        }
    }

    fn help(&self) -> Option<Cow<'_, str>> {
        Some(Cow::Borrowed(match self.kind {
            Kind::NotACollection { .. } => {
                "Open a directory with a `pawsman.toml`, or create one with `format = 1`."
            }
            Kind::InvalidFile { .. } => "Fix the file; the other requests still work.",
        }))
    }
}
