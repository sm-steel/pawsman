//! Command-line flags.

use clap::Parser;
use pawsman_core::settings::ImageMode;

/// A terminal API client.
#[derive(Debug, Parser)]
#[command(name = "pawsman", version, about)]
pub(crate) struct Args {
    /// Start without the startup animation.
    #[arg(long)]
    pub(crate) no_splash: bool,
    /// How images are drawn: auto, kitty, sixel, blocks or off.
    #[arg(long, value_name = "MODE")]
    pub(crate) images: Option<ImageMode>,
}
