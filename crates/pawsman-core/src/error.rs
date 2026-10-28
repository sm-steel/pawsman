//! The core crate's error.

use std::borrow::Cow;
use std::fmt;
use std::path::{Path, PathBuf};

use pawsman_error::{ErrorCode, ErrorInfo, Retry};

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
    GlobalsInvalid { path: PathBuf, reason: String },
    BodyFile { path: PathBuf, reason: String },
    Write { path: PathBuf, reason: String },
    Read { path: PathBuf, reason: String },
    Exists { path: PathBuf },
    Watch { reason: String },
}

impl CoreError {
    pub(crate) fn no_home() -> Self {
        Self { kind: Kind::NoHome }
    }

    pub(crate) fn globals_invalid(path: &Path, reason: impl Into<String>) -> Self {
        Self {
            kind: Kind::GlobalsInvalid {
                path: path.to_owned(),
                reason: reason.into(),
            },
        }
    }

    pub(crate) fn body_file(path: &Path, err: &std::io::Error) -> Self {
        Self {
            kind: Kind::BodyFile {
                path: path.to_owned(),
                reason: err.to_string(),
            },
        }
    }

    pub(crate) fn write(path: &Path, err: &std::io::Error) -> Self {
        Self {
            kind: Kind::Write {
                path: path.to_owned(),
                reason: err.to_string(),
            },
        }
    }

    pub(crate) fn read(path: &Path, err: &std::io::Error) -> Self {
        Self {
            kind: Kind::Read {
                path: path.to_owned(),
                reason: err.to_string(),
            },
        }
    }

    pub(crate) fn exists(path: &Path) -> Self {
        Self {
            kind: Kind::Exists {
                path: path.to_owned(),
            },
        }
    }

    pub(crate) fn watch(err: &notify::Error) -> Self {
        Self {
            kind: Kind::Watch {
                reason: err.to_string(),
            },
        }
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
            Kind::BodyFile { path, reason } => {
                write!(
                    f,
                    "`{}` cannot be read for the body: {reason}",
                    path.display()
                )
            }
            Kind::Write { path, reason } => {
                write!(f, "`{}` cannot be written: {reason}", path.display())
            }
            Kind::Read { path, reason } => {
                write!(f, "`{}` cannot be read: {reason}", path.display())
            }
            Kind::Exists { path } => write!(f, "`{}` already exists", path.display()),
            Kind::Watch { reason } => {
                write!(f, "edits made outside pawsman are not watched: {reason}")
            }
            Kind::SettingsInvalid { path, reason } | Kind::GlobalsInvalid { path, reason } => {
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
            Kind::GlobalsInvalid { .. } => codes::GLOBALS_INVALID,
            Kind::BodyFile { .. } => codes::BODY_FILE,
            Kind::Write { .. } => codes::WRITE,
            Kind::Read { .. } => codes::READ,
            Kind::Exists { .. } => codes::EXISTS,
            Kind::Watch { .. } => codes::WATCH,
        }
    }

    fn help(&self) -> Option<Cow<'_, str>> {
        Some(Cow::Borrowed(match self.kind {
            Kind::NoHome => "Set `HOME` to your home directory.",
            Kind::SettingsInvalid { .. } => "Fix the file, or remove it to use the defaults.",
            Kind::GlobalsInvalid { .. } => {
                "Fix the file, or remove it; collections still open without it."
            }
            Kind::BodyFile { .. } => {
                "Check the path in the request's `[body]`; it is relative to the request file."
            }
            Kind::Write { .. } => "Check the folder's permissions and free space, then retry.",
            Kind::Read { .. } => "Check the file's permissions, then save it again.",
            Kind::Exists { .. } => "Pick another name, or move the other file away first.",
            Kind::Watch { .. } => "Press Ctrl-R to reload after editing files elsewhere.",
        }))
    }

    fn retry(&self) -> Retry {
        match self.kind {
            Kind::Write { .. } | Kind::Read { .. } => Retry::Retryable,
            _ => Retry::Permanent,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    fn errors() -> Vec<CoreError> {
        vec![
            CoreError::no_home(),
            CoreError::settings_invalid(Path::new("/x/settings.toml"), "unknown field `y`"),
        ]
    }

    #[test]
    fn help_is_one_imperative_sentence() {
        for err in errors() {
            let help = err.help().expect("every core error has help");
            assert!(help.ends_with('.'), "ends with a period: {help}");
            assert!(
                help.starts_with(char::is_uppercase),
                "starts with a verb: {help}"
            );
        }
    }

    #[test]
    fn messages_name_the_input() {
        let [home, settings] = errors().try_into().unwrap();

        assert!(home.to_string().contains("`HOME`"), "{home}");
        assert_eq!(
            settings.to_string(),
            "`/x/settings.toml` is not valid: unknown field `y`",
            "file and reason"
        );
    }
}
