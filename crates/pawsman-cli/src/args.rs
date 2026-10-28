//! Command-line flags.

use std::path::PathBuf;

use clap::{Parser, Subcommand};
use pawsman_core::settings::ImageMode;

/// A terminal API client.
#[derive(Debug, Parser)]
#[command(
    name = "pawsman",
    version,
    about,
    args_conflicts_with_subcommands = true
)]
pub(crate) struct Args {
    #[command(subcommand)]
    pub(crate) command: Option<Command>,
    /// The collection directory to open.
    pub(crate) dir: Option<PathBuf>,
    /// Start without the startup animation.
    #[arg(long)]
    pub(crate) no_splash: bool,
    /// How images are drawn: auto, kitty, sixel, blocks or off.
    #[arg(long, value_name = "MODE")]
    pub(crate) images: Option<ImageMode>,
}

/// What to do instead of starting the TUI.
#[derive(Debug, Subcommand)]
pub(crate) enum Command {
    /// Create a collection: `pawsman.toml` and `.gitignore` lines for `.env` and `.pawsman/`.
    Init {
        /// The directory, created if missing.
        #[arg(default_value = ".")]
        dir: PathBuf,
    },
}
