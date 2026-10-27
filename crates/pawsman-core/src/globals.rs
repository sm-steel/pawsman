//! `~/.config/pawsman/globals.toml`: variables every collection sees, below the collection's own.

use std::path::Path;

use crate::CoreError;

/// Reads the global variables; a missing file has none.
///
/// # Errors
///
/// `core::globals_invalid` when the file cannot be read or parsed.
pub fn load(path: &Path) -> Result<Vec<(String, String)>, CoreError> {
    let _ = path;
    Err(CoreError::globals_invalid(path, "todo"))
}

#[cfg(test)]
mod tests {
    use pawsman_error::ErrorInfo as _;

    use super::*;

    #[test]
    fn missing_globals_are_empty() {
        let dir = tempfile::tempdir().unwrap();

        assert_eq!(load(&dir.path().join("globals.toml")).unwrap(), vec![]);
    }

    #[test]
    fn globals_vars_load_in_order() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("globals.toml");
        std::fs::write(&path, "[vars]\nz = \"1\"\na = \"2\"\n").unwrap();

        assert_eq!(
            load(&path).unwrap(),
            vec![
                ("z".to_owned(), "1".to_owned()),
                ("a".to_owned(), "2".to_owned())
            ]
        );
    }

    #[test]
    fn bad_globals_are_globals_invalid() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("globals.toml");
        for text in ["vars = 1\n", "[vars]\nport = 1\n", "[other]\n", "[vars\n"] {
            std::fs::write(&path, text).unwrap();

            let err = load(&path).unwrap_err();

            assert_eq!(err.code(), crate::codes::GLOBALS_INVALID, "{text:?}");
            assert!(err.to_string().contains("globals.toml"), "{err}");
        }
    }
}
