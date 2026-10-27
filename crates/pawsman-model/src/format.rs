//! The on-disk format version and migrations between versions.

use std::path::Path;

use crate::ModelError;

/// The newest format this build reads and writes.
pub const CURRENT: u32 = 1;

/// Brings `table`, written in format `from`, up to [`CURRENT`]. The single entry point for every
/// file of a collection.
///
/// # Errors
///
/// `model::format_too_new` for a format newer than [`CURRENT`]; `model::invalid_file` for `0`.
pub fn migrate(table: toml::Table, from: u32, path: &Path) -> Result<toml::Table, ModelError> {
    match from {
        0 => Err(ModelError::invalid_file(path, "`format` must be 1 or more")),
        CURRENT => Ok(table),
        _ => Err(ModelError::format_too_new(path, from)),
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use pawsman_error::ErrorInfo as _;

    use super::*;
    use crate::codes;

    #[test]
    fn migrate_is_the_identity_at_the_current_format() {
        let table: toml::Table = toml::from_str("name = \"x\"\n").unwrap();

        assert_eq!(
            migrate(table.clone(), CURRENT, Path::new("m")).unwrap(),
            table
        );
    }

    #[test]
    fn newer_is_too_new_and_zero_is_invalid() {
        let newer = migrate(toml::Table::new(), CURRENT + 1, Path::new("m")).unwrap_err();
        let zero = migrate(toml::Table::new(), 0, Path::new("m")).unwrap_err();

        assert_eq!(newer.code(), codes::FORMAT_TOO_NEW, "newer");
        assert_eq!(zero.code(), codes::INVALID_FILE, "zero");
    }
}
