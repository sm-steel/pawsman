//! User settings from `settings.toml`. Unknown keys are an error; the caller shows it and uses the
//! defaults.

use std::fmt;
use std::path::Path;
use std::str::FromStr;

use serde::Deserialize;

use crate::CoreError;

/// Everything in `settings.toml`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct Settings {
    ui: Ui,
}

/// The `[ui]` table.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct Ui {
    animations: Animations,
    images: ImageMode,
}

/// Whether the startup animation and other motion play.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(from = "bool")]
pub enum Animations {
    /// Play them.
    #[default]
    On,
    /// Show final frames only.
    Off,
}

impl From<bool> for Animations {
    fn from(on: bool) -> Self {
        if on { Self::On } else { Self::Off }
    }
}

/// How images are drawn; `Auto` follows the probe.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ImageMode {
    /// Pick from the probed capabilities.
    #[default]
    Auto,
    /// Kitty graphics protocol through Unicode placeholders.
    Kitty,
    /// Sixel.
    Sixel,
    /// Half blocks.
    Blocks,
    /// A text placeholder.
    Off,
}

impl ImageMode {
    const NAMES: [(&'static str, Self); 5] = [
        ("auto", Self::Auto),
        ("kitty", Self::Kitty),
        ("sixel", Self::Sixel),
        ("blocks", Self::Blocks),
        ("off", Self::Off),
    ];
}

impl fmt::Display for ImageMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = Self::NAMES
            .iter()
            .find(|(_, mode)| mode == self)
            .map_or("auto", |(name, _)| name);
        f.write_str(name)
    }
}

/// A value that is not an image mode.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnknownImageMode(String);

impl fmt::Display for UnknownImageMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "unknown image mode `{}`, expected auto, kitty, sixel, blocks or off",
            self.0
        )
    }
}

impl std::error::Error for UnknownImageMode {}

impl FromStr for ImageMode {
    type Err = UnknownImageMode;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::NAMES
            .iter()
            .find(|(name, _)| *name == s)
            .map(|(_, mode)| *mode)
            .ok_or_else(|| UnknownImageMode(s.to_owned()))
    }
}

impl Settings {
    /// Reads `path`; a missing file gives the defaults.
    ///
    /// # Errors
    ///
    /// `core::settings_invalid` when the file cannot be read or parsed.
    pub fn load(path: &Path) -> Result<Self, CoreError> {
        let text = match std::fs::read_to_string(path) {
            Ok(text) => text,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(err) => return Err(CoreError::settings_invalid(path, err.to_string())),
        };
        toml::from_str(&text)
            .map_err(|err| CoreError::settings_invalid(path, err.message().replace('\n', " ")))
    }

    /// The `[ui]` table.
    #[must_use]
    pub fn ui(&self) -> &Ui {
        &self.ui
    }
}

impl Ui {
    /// `animations`.
    #[must_use]
    pub fn animations(&self) -> Animations {
        self.animations
    }

    /// `images`.
    #[must_use]
    pub fn images(&self) -> ImageMode {
        self.images
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use pawsman_error::ErrorInfo as _;

    use super::*;

    fn write(text: &str) -> tempfile::NamedTempFile {
        let mut file = tempfile::NamedTempFile::new().unwrap();
        file.write_all(text.as_bytes()).unwrap();
        file
    }

    #[test]
    fn missing_file_gives_defaults() {
        let dir = tempfile::tempdir().unwrap();

        let settings = Settings::load(&dir.path().join("settings.toml")).unwrap();

        assert_eq!(settings, Settings::default(), "defaults");
        assert_eq!(
            settings.ui().animations(),
            Animations::On,
            "animations on by default"
        );
        assert_eq!(
            settings.ui().images(),
            ImageMode::Auto,
            "images auto by default"
        );
    }

    #[test]
    fn an_unreadable_file_is_settings_invalid() {
        let dir = tempfile::tempdir().unwrap();

        let err = Settings::load(dir.path()).unwrap_err();

        assert_eq!(
            err.code(),
            crate::codes::SETTINGS_INVALID,
            "a directory is not a file"
        );
    }

    #[test]
    fn ui_values_are_read() {
        let file = write("[ui]\nanimations = false\nimages = \"blocks\"\n");

        let settings = Settings::load(file.path()).unwrap();

        assert_eq!(
            settings.ui().animations(),
            Animations::Off,
            "animations off"
        );
        assert_eq!(settings.ui().images(), ImageMode::Blocks, "images blocks");
    }

    #[test]
    fn unknown_key_is_settings_invalid_naming_the_file() {
        let file = write("[ui]\nanimation = false\n");

        let err = Settings::load(file.path()).unwrap_err();

        assert_eq!(err.code(), crate::codes::SETTINGS_INVALID, "code");
        assert!(
            err.to_string().contains("animation"),
            "names the key: {err}"
        );
        assert!(!err.to_string().contains('\n'), "one line: {err}");
    }

    #[test]
    fn image_mode_parses_and_rejects() {
        assert_eq!(
            "sixel".parse::<ImageMode>().unwrap(),
            ImageMode::Sixel,
            "sixel"
        );
        let err = "png".parse::<ImageMode>().unwrap_err();
        assert!(err.to_string().contains("`png`"), "names the value: {err}");
    }

    #[test]
    fn image_mode_round_trips_through_display() {
        for (name, mode) in ImageMode::NAMES {
            assert_eq!(mode.to_string(), name, "display of {mode:?}");
        }
    }
}
