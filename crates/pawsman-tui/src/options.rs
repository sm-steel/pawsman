//! What the binary hands the TUI.

use std::path::PathBuf;

use pawsman_core::engine::Setup;
use pawsman_core::env::EnvSnapshot;
use pawsman_core::keys::Keymap;
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
    globals: Option<PathBuf>,
    settings: Settings,
    keymap: Keymap,
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
            globals: None,
            settings: Settings::default(),
            keymap: Keymap::default(),
        }
    }

    /// The key bindings, `keys.toml` applied.
    #[must_use]
    pub fn keymap(mut self, keymap: Keymap) -> Self {
        self.keymap = keymap;
        self
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

    /// `globals.toml`.
    #[must_use]
    pub fn globals(mut self, globals: Option<PathBuf>) -> Self {
        self.globals = globals;
        self
    }

    /// What the engine starts with; takes the collection directory.
    pub(crate) fn engine_setup(&mut self) -> Setup {
        Setup::new(self.settings.clone())
            .dir(self.dir.take())
            .env(self.env.clone())
            .globals(self.globals.clone())
    }

    /// Notices to show once the main view is up.
    #[must_use]
    pub fn notices(mut self, notices: Vec<Notice>) -> Self {
        self.notices = notices;
        self
    }

    pub(crate) fn keys(&self) -> &Keymap {
        &self.keymap
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
