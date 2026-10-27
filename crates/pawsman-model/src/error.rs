//! The model crate's error.

use std::borrow::Cow;
use std::fmt;
use std::path::{Path, PathBuf};

use pawsman_error::{ErrorCode, ErrorInfo};

use crate::codes;
use crate::template::TemplateError;

/// An error from `pawsman-model`.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct ModelError {
    kind: Kind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Kind {
    NotACollection {
        dir: PathBuf,
    },
    InvalidFile {
        path: PathBuf,
        reason: String,
    },
    FormatTooNew {
        path: PathBuf,
        format: u32,
    },
    MissingSidecar {
        path: PathBuf,
        name: String,
    },
    BadTemplate {
        field: String,
        at: usize,
        reason: &'static str,
    },
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

    /// `path` uses a format newer than this build reads.
    #[must_use]
    pub fn format_too_new(path: &Path, format: u32) -> Self {
        Self {
            kind: Kind::FormatTooNew {
                path: path.to_owned(),
                format,
            },
        }
    }

    /// The request at `path` names the sidecar `name`, which does not exist.
    #[must_use]
    pub fn missing_sidecar(path: &Path, name: &str) -> Self {
        Self {
            kind: Kind::MissingSidecar {
                path: path.to_owned(),
                name: name.to_owned(),
            },
        }
    }

    /// `field` holds a template that does not parse.
    #[must_use]
    pub fn bad_template(field: &str, err: &TemplateError) -> Self {
        Self {
            kind: Kind::BadTemplate {
                field: field.to_owned(),
                at: err.span.start,
                reason: err.reason,
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
            Kind::FormatTooNew { path, format } => write!(
                f,
                "`{}` uses format {format}; this pawsman reads up to {}",
                path.display(),
                crate::format::CURRENT
            ),
            Kind::MissingSidecar { path, name } => write!(
                f,
                "`{}` names the body file `{name}`, which does not exist",
                path.display()
            ),
            Kind::BadTemplate { field, at, reason } => {
                write!(f, "`{field}` has a bad template at {at}: {reason}")
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
            Kind::FormatTooNew { .. } => codes::FORMAT_TOO_NEW,
            Kind::MissingSidecar { .. } => codes::MISSING_SIDECAR,
            Kind::BadTemplate { .. } => codes::BAD_TEMPLATE,
        }
    }

    fn help(&self) -> Option<Cow<'_, str>> {
        Some(Cow::Borrowed(match self.kind {
            Kind::NotACollection { .. } => {
                "Open a directory with a `pawsman.toml`, or create one with `format = 1`."
            }
            Kind::InvalidFile { .. } => "Fix the file; the other requests still work.",
            Kind::FormatTooNew { .. } => {
                "Update pawsman; this collection was written by a newer version."
            }
            Kind::MissingSidecar { .. } => {
                "Restore the body file next to the request, or put the body inline."
            }
            Kind::BadTemplate { .. } => {
                "Close every `{{` with `}}`, or write `\\{{` for a literal `{{`."
            }
        }))
    }
}
