//! What pawsman remembers about a collection between runs, in `.pawsman/state.toml`: the active
//! environment.

use std::path::Path;

use serde::Deserialize;
use tracing::debug;

use crate::{CoreError, store};

const FILE: &str = ".pawsman/state.toml";

#[derive(Deserialize)]
struct State {
    env: Option<String>,
}

/// The environment active when pawsman last closed `root`; `None` when nothing is remembered.
#[must_use]
pub fn load(root: &Path) -> Option<String> {
    let text = std::fs::read_to_string(root.join(FILE)).ok()?;
    match toml::from_str::<State>(&text) {
        Ok(state) => state.env,
        Err(err) => {
            debug!(%err, "state.toml ignored");
            None
        }
    }
}

/// Remembers `env` as the active environment of `root`.
///
/// # Errors
///
/// `core::write` when `.pawsman/state.toml` cannot be written.
pub fn save(root: &Path, env: Option<&str>) -> Result<(), CoreError> {
    let text = env
        .map(|env| format!("env = {}\n", toml::Value::String(env.to_owned())))
        .unwrap_or_default();
    store::write(root, Path::new(FILE), text.as_bytes())
}
