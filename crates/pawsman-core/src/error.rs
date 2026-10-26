//! The core crate's error.

use std::borrow::Cow;
use std::fmt;
use std::path::{Path, PathBuf};

use pawsman_error::{ErrorCode, ErrorInfo};

use crate::codes;

/// An error from `pawsman-core`.
#[derive(Debug)]
#[non_exhaustive]
pub struct CoreError {
    kind: Kind,
}

#[derive(Debug)]
enum Kind {
    NoHome,
    SettingsInvalid { path: PathBuf, reason: String },
}

impl CoreError {
    pub(crate) fn no_home() -> Self {
        Self { kind: Kind::NoHome }
    }

    pub(crate) fn settings_invalid(path: &Path, reason: impl Into<String>) -> Self {
        Self {
            kind: Kind::SettingsInvalid {
                path: path.to_owned(),
                reason: reason.into(),
            },
        }
    }
}

impl fmt::Display for CoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.kind {
            Kind::NoHome => f.write_str("neither `HOME` nor the `XDG_*` directories are set"),
            Kind::SettingsInvalid { path, reason } => {
                write!(f, "`{}` is not valid: {reason}", path.display())
            }
        }
    }
}

impl std::error::Error for CoreError {}

impl ErrorInfo for CoreError {
    fn code(&self) -> ErrorCode {
        match self.kind {
            Kind::NoHome => codes::NO_HOME,
            Kind::SettingsInvalid { .. } => codes::SETTINGS_INVALID,
        }
    }

    fn help(&self) -> Option<Cow<'_, str>> {
        Some(Cow::Borrowed(match self.kind {
            Kind::NoHome => "Set `HOME` to your home directory.",
            Kind::SettingsInvalid { .. } => "Fix the file, or remove it to use the defaults.",
        }))
    }
}
