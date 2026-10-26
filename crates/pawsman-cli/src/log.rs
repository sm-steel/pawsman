//! The rolling file log; never the terminal (OBSERVABILITY P2–P4).

use std::path::Path;

use pawsman_core::env::EnvSnapshot;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_appender::rolling::{Builder, Rotation};
use tracing_subscriber::EnvFilter;

const DEFAULT_FILTER: &str = "warn,pawsman=info";

/// Installs the global subscriber writing to `dir/pawsman.<date>.log`; the guard flushes on drop.
pub(crate) fn install(dir: &Path, env: &EnvSnapshot) -> Result<WorkerGuard, String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("`{}`: {e}", dir.display()))?;
    let appender = Builder::new()
        .rotation(Rotation::DAILY)
        .filename_prefix("pawsman")
        .filename_suffix("log")
        .max_log_files(14)
        .build(dir)
        .map_err(|e| e.to_string())?;
    let (writer, guard) = tracing_appender::non_blocking(appender);
    let filter = env
        .log_filter()
        .and_then(|f| EnvFilter::try_new(f).ok())
        .unwrap_or_else(|| EnvFilter::new(DEFAULT_FILTER));
    tracing_subscriber::fmt()
        .with_writer(writer)
        .with_ansi(false)
        .with_env_filter(filter)
        .try_init()
        .map_err(|e| e.to_string())?;
    Ok(guard)
}
