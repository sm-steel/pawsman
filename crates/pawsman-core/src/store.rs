//! Reading collections from disk. The model parses; this reads the files.

use std::path::Path;

use pawsman_model::{Collection, ModelError, SourceFile};
use tracing::{info, warn};

/// Loads the collection in `dir`: `pawsman.toml` and every `*.req.toml` directly in it.
///
/// # Errors
///
/// `model::not_a_collection` when `dir` or its `pawsman.toml` is missing; `model::invalid_file`
/// for a bad manifest. A request file that does not parse becomes an error entry; one that cannot
/// be read is logged and skipped.
pub fn load(dir: &Path) -> Result<Collection, ModelError> {
    let mut files = Vec::new();
    if let Ok(bytes) = std::fs::read(dir.join("pawsman.toml")) {
        files.push(SourceFile::new("pawsman.toml".into(), bytes));
    }
    let mut unreadable = Vec::new();
    if let Ok(read_dir) = std::fs::read_dir(dir) {
        for entry in read_dir.flatten() {
            let path = entry.path();
            let is_request = path
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.ends_with(".req.toml"));
            if !is_request || !path.is_file() {
                continue;
            }
            match std::fs::read(&path) {
                Ok(bytes) => files.push(SourceFile::new(entry.file_name().into(), bytes)),
                Err(err) => unreadable.push(ModelError::unreadable(&path, &err)),
            }
        }
    }
    let collection = Collection::from_files(dir, files)?;
    for err in &unreadable {
        warn!(%err, "request file unreadable");
    }
    info!(
        dir = %dir.display(),
        requests = collection.entries().len(),
        "collection loaded"
    );
    Ok(collection)
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
