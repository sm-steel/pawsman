//! What the binary hands the TUI.

use std::path::PathBuf;

use pawsman_core::env::EnvSnapshot;
use pawsman_core::notice::Notice;
use pawsman_core::settings::{ImageMode, Settings};

use crate::SplashSetting;

/// Options for [`run`](crate::run).
#[derive(Debug)]
pub struct Options {
    env: EnvSnapshot,
    splash: SplashSetting,
    images: ImageMode,
    notices: Vec<Notice>,
    dir: Option<PathBuf>,
    settings: Settings,
}

impl Options {
    /// Options from the env snapshot, the splash choice and the image mode.
    #[must_use]
    pub fn new(env: EnvSnapshot, splash: SplashSetting, images: ImageMode) -> Self {
        Self {
            env,
            splash,
            images,
            notices: Vec::new(),
            dir: None,
            settings: Settings::default(),
        }
    }

    /// The collection directory to open.
    #[must_use]
    pub fn dir(mut self, dir: Option<PathBuf>) -> Self {
        self.dir = dir;
        self
    }

    /// The loaded settings.
    #[must_use]
    pub fn settings(mut self, settings: Settings) -> Self {
        self.settings = settings;
        self
    }

    pub(crate) fn take_dir(&mut self) -> Option<PathBuf> {
        self.dir.take()
    }

    pub(crate) fn settings_ref(&self) -> &Settings {
        &self.settings
    }

    /// Notices to show once the main view is up.
    #[must_use]
    pub fn notices(mut self, notices: Vec<Notice>) -> Self {
        self.notices = notices;
        self
    }

    pub(crate) fn env(&self) -> &EnvSnapshot {
        &self.env
    }

    pub(crate) fn splash(&self) -> SplashSetting {
        self.splash
    }

    pub(crate) fn images(&self) -> ImageMode {
        self.images
    }

    pub(crate) fn into_notices(self) -> Vec<Notice> {
        self.notices
    }
}
