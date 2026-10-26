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
