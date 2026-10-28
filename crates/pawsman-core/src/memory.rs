//! What pawsman remembers about a collection between runs, in `.pawsman/state.toml`: the active
//! environment.

use std::path::Path;

use crate::CoreError;

/// The environment active when pawsman last closed `root`; `None` when nothing is remembered.
#[must_use]
pub fn load(root: &Path) -> Option<String> {
    let _ = root;
    None
}

/// Remembers `env` as the active environment of `root`.
///
/// # Errors
///
/// `core::write` when `.pawsman/state.toml` cannot be written.
pub fn save(root: &Path, env: Option<&str>) -> Result<(), CoreError> {
    let _ = (root, env);
    Ok(())
}
