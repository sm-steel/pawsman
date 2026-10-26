//! Where pawsman keeps configuration and state: `XDG_*` when absolute, else under `$HOME`, on Linux
//! and macOS alike.

use std::path::{Path, PathBuf};

use crate::CoreError;
use crate::env::EnvSnapshot;

/// pawsman's directories.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Paths {
    config: PathBuf,
    state: PathBuf,
}

impl Paths {
    /// Resolves the directories from the env snapshot.
    ///
    /// # Errors
    ///
    /// `core::no_home` when a directory has neither an absolute `XDG_*` value nor `HOME`.
    pub fn from_env(env: &EnvSnapshot) -> Result<Self, CoreError> {
        let dir = |xdg: &str, fallback: &str| -> Result<PathBuf, CoreError> {
            let base = env
                .var(xdg)
                .map(PathBuf::from)
                .filter(|p| p.is_absolute())
                .or_else(|| env.var("HOME").map(|home| Path::new(home).join(fallback)))
                .ok_or_else(CoreError::no_home)?;
            Ok(base.join("pawsman"))
        };
        Ok(Self {
            config: dir("XDG_CONFIG_HOME", ".config")?,
            state: dir("XDG_STATE_HOME", ".local/state")?,
        })
    }

    /// `…/pawsman` under the config directory.
    #[must_use]
    pub fn config_dir(&self) -> &Path {
        &self.config
    }

    /// `…/pawsman` under the state directory.
    #[must_use]
    pub fn state_dir(&self) -> &Path {
        &self.state
    }

    /// `settings.toml` in the config directory.
    #[must_use]
    pub fn settings_file(&self) -> PathBuf {
        self.config.join("settings.toml")
    }

    /// Where the rolling log goes.
    #[must_use]
    pub fn log_dir(&self) -> &Path {
        &self.state
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use pawsman_error::ErrorInfo as _;

    use super::*;

    #[test]
    fn xdg_directories_win_over_home() {
        let env = EnvSnapshot::from_pairs([
            ("HOME", "/home/u"),
            ("XDG_CONFIG_HOME", "/cfg"),
            ("XDG_STATE_HOME", "/st"),
        ]);

        let paths = Paths::from_env(&env).unwrap();

        assert_eq!(
            paths.config_dir(),
            Path::new("/cfg/pawsman"),
            "XDG_CONFIG_HOME"
        );
        assert_eq!(
            paths.state_dir(),
            Path::new("/st/pawsman"),
            "XDG_STATE_HOME"
        );
        assert_eq!(
            paths.settings_file(),
            Path::new("/cfg/pawsman/settings.toml"),
            "settings file"
        );
    }

    #[test]
    fn home_is_the_fallback_and_relative_xdg_is_ignored() {
        let env = EnvSnapshot::from_pairs([("HOME", "/home/u"), ("XDG_CONFIG_HOME", "relative")]);

        let paths = Paths::from_env(&env).unwrap();

        assert_eq!(
            paths.config_dir(),
            Path::new("/home/u/.config/pawsman"),
            "relative XDG ignored"
        );
        assert_eq!(
            paths.log_dir(),
            Path::new("/home/u/.local/state/pawsman"),
            "state under HOME"
        );
    }

    #[test]
    fn nothing_set_is_no_home() {
        let err = Paths::from_env(&EnvSnapshot::default()).unwrap_err();

        assert_eq!(err.code(), crate::codes::NO_HOME, "code");
    }
}
