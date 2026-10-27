//! `pawsman`: the TUI.
#![forbid(unsafe_code)]

mod args;
mod log;

use std::io::Write as _;
use std::process::ExitCode;

use clap::Parser as _;
use pawsman_core::env::EnvSnapshot;
use pawsman_core::notice::Notice;
use pawsman_core::paths::Paths;
use pawsman_core::settings::{Animations, Settings};
use pawsman_error::ErrorInfo as _;
use pawsman_tui::{Options, SplashSetting, TuiError};

fn main() -> ExitCode {
    let args = args::Args::parse();
    let env = EnvSnapshot::capture();
    let mut notices = Vec::new();
    let paths = Paths::from_env(&env)
        .inspect_err(|e| notices.push(Notice::from_error(e)))
        .ok();
    let _guard = paths.as_ref().and_then(|p| {
        log::install(p.log_dir(), &env)
            .inspect_err(|e| notices.push(Notice::warn(format!("no log file: {e}"))))
            .ok()
    });
    tracing::info!(
        version = env!("CARGO_PKG_VERSION"),
        term = env.term().unwrap_or("-"),
        tmux = env.in_tmux(),
        ssh = env.over_ssh(),
        utf8 = env.utf8_locale(),
        "start"
    );
    let settings = match paths.as_ref().map(|p| Settings::load(&p.settings_file())) {
        Some(Ok(settings)) => settings,
        Some(Err(err)) => {
            notices.push(Notice::from_error(&err));
            Settings::default()
        }
        None => Settings::default(),
    };
    let splash =
        if args.no_splash || settings.ui().animations() == Animations::Off || env.no_animation() {
            SplashSetting::Off
        } else {
            SplashSetting::On
        };
    let images = args.images.unwrap_or_else(|| settings.ui().images());
    let options = Options::new(env, splash, images)
        .notices(notices)
        .dir(args.dir)
        .settings(settings)
        .globals(paths.as_ref().map(Paths::globals_file));
    match pawsman_tui::run(options) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            tracing::error!(code = %err.code(), %err, "exit");
            exit_line(&err);
            ExitCode::FAILURE
        }
    }
}

/// The one line printed after the terminal is restored.
#[expect(
    clippy::disallowed_methods,
    reason = "the exit line after restore (rules/tui.md §1)"
)]
fn exit_line(err: &TuiError) {
    let help = err.help().map(|h| format!(" {h}")).unwrap_or_default();
    let _ = writeln!(std::io::stderr(), "pawsman: {}: {err}.{help}", err.code());
}
