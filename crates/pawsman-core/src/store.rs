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

    fn put(dir: &Path, rel: &str, text: &str) {
        let path = dir.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    fn paths(c: &pawsman_model::Collection) -> Vec<String> {
        c.entries()
            .iter()
            .map(|e| e.path().display().to_string())
            .collect()
    }

    const MANIFEST: &str = "format = 1\nname = \"D\"\n";
    const REQ: &str = "name = \"r\"\nurl = \"http://h\"\n";

    #[test]
    fn loads_the_tree_with_folders_envs_and_sidecars() {
        let dir = tempfile::tempdir().unwrap();
        put(dir.path(), "pawsman.toml", MANIFEST);
        put(dir.path(), "users/_folder.toml", "name = \"Users\"\n");
        put(
            dir.path(),
            "users/create.req.toml",
            "name = \"c\"\nurl = \"u\"\n[body]\njson = { sidecar = \"create.body.json\" }\n",
        );
        put(dir.path(), "users/create.body.json", "{}");
        put(
            dir.path(),
            "envs/dev.env.toml",
            "[vars]\nbase = \"http://d\"\n",
        );

        let c = load(dir.path()).unwrap();

        assert_eq!(paths(&c), ["users/create.req.toml"], "nested request");
        assert!(
            matches!(c.entries()[0].kind(), EntryKind::Request(_)),
            "sidecar found: {:?}",
            c.entries()[0].kind()
        );
        assert_eq!(c.environments().len(), 1, "environment");
        assert_eq!(c.chain(c.entries()[0].id()).len(), 2, "root and users");
    }

    #[test]
    fn dotenv_is_read() {
        let dir = tempfile::tempdir().unwrap();
        put(dir.path(), "pawsman.toml", MANIFEST);
        put(dir.path(), ".env", "token=abc\n");

        let c = load(dir.path()).unwrap();

        assert_eq!(c.dotenv().len(), 1, "values");
    }

    #[cfg(unix)]
    #[test]
    fn symlinked_dirs_and_hidden_dirs_are_skipped() {
        let dir = tempfile::tempdir().unwrap();
        put(dir.path(), "pawsman.toml", MANIFEST);
        put(dir.path(), "a.req.toml", REQ);
        put(dir.path(), ".git/x.req.toml", REQ);
        put(dir.path(), ".pawsman/y.req.toml", REQ);
        put(dir.path(), "node_modules/z.req.toml", REQ);
        put(dir.path(), "target/t.req.toml", REQ);
        std::os::unix::fs::symlink(dir.path(), dir.path().join("loop")).unwrap();

        let c = load(dir.path()).unwrap();

        assert_eq!(paths(&c), ["a.req.toml"], "only the real request");
    }

    #[test]
    fn depth_is_capped() {
        let dir = tempfile::tempdir().unwrap();
        put(dir.path(), "pawsman.toml", MANIFEST);
        let mut rel = String::new();
        for level in 0..20 {
            rel.push_str(&format!("d{level}/"));
            put(dir.path(), &format!("{rel}r.req.toml"), REQ);
        }
        let started = std::time::Instant::now();

        let c = load(dir.path()).unwrap();

        assert_eq!(
            c.entries().len(),
            16,
            "levels 1 to 16 load: {:?}",
            paths(&c)
        );
        assert!(
            started.elapsed() < std::time::Duration::from_secs(2),
            "fast"
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
