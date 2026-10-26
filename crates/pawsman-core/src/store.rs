//! Reading collections from disk. The model parses; this reads the files.

use std::path::Path;

use pawsman_model::{Collection, ModelError};

/// Loads the collection in `dir`: `pawsman.toml` and every `*.req.toml` directly in it.
///
/// # Errors
///
/// `model::not_a_collection` when `dir` or its `pawsman.toml` is missing; `model::invalid_file`
/// for a bad manifest. A request file that cannot be read or parsed becomes an error entry.
pub fn load(dir: &Path) -> Result<Collection, ModelError> {
    Collection::from_files(dir, None, Vec::new())
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use pawsman_error::ErrorInfo as _;
    use pawsman_model::EntryKind;

    use super::*;

    fn write(dir: &Path, name: &str, text: &str) {
        std::fs::write(dir.join(name), text).unwrap();
    }

    #[test]
    fn loads_requests_and_ignores_other_files() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "pawsman.toml", "format = 1\nname = \"D\"\n");
        write(
            dir.path(),
            "a.req.toml",
            "name = \"A\"\nurl = \"http://h\"\n",
        );
        write(dir.path(), "notes.md", "# not a request");
        std::fs::create_dir(dir.path().join("sub")).unwrap();

        let c = load(dir.path()).unwrap();

        assert_eq!(c.name(), "D", "manifest read");
        assert_eq!(c.entries().len(), 1, "only *.req.toml");
        assert!(
            matches!(c.entries()[0].kind(), EntryKind::Request(_)),
            "parsed"
        );
    }

    #[test]
    fn a_directory_without_a_manifest_is_not_a_collection() {
        let dir = tempfile::tempdir().unwrap();
        write(
            dir.path(),
            "a.req.toml",
            "name = \"A\"\nurl = \"http://h\"\n",
        );

        let err = load(dir.path()).unwrap_err();

        assert_eq!(err.code(), pawsman_model::codes::NOT_A_COLLECTION, "code");
    }

    #[test]
    fn a_missing_directory_is_not_a_collection() {
        let err = load(Path::new("/nonexistent/pawsman")).unwrap_err();

        assert_eq!(err.code(), pawsman_model::codes::NOT_A_COLLECTION, "code");
    }
}
