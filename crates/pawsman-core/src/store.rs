//! Reading collections from disk. The model parses; this walks the directory and reads bytes.

use std::path::{Path, PathBuf};
use std::time::Instant;

use pawsman_model::{Collection, ModelError, Node, SourceFile};
use tracing::{info, warn};

/// Folders below this depth are not read.
const MAX_DEPTH: usize = 16;
/// At most this many files are read.
const MAX_FILES: usize = 10_000;
/// Directories never read, besides hidden ones.
const SKIPPED: [&str; 2] = ["node_modules", "target"];

/// Loads the collection in `dir`: `pawsman.toml`, folders, requests, sidecars, environments and
/// `.env`. Hidden directories, `node_modules` and `target` are skipped, symlinked directories are
/// not followed, and the walk stops at 16 levels and 10 000 files.
///
/// # Errors
///
/// `model::not_a_collection` when `dir` or its `pawsman.toml` is missing; `model::invalid_file`
/// or `model::format_too_new` for a bad manifest. Broken files inside become error entries; files
/// that cannot be read are logged and skipped.
pub fn load(dir: &Path) -> Result<Collection, ModelError> {
    let started = Instant::now();
    let mut files = Vec::new();
    let mut stack = vec![(PathBuf::new(), 0)];
    let (mut too_deep, mut too_many) = (false, false);
    while let Some((rel, depth)) = stack.pop() {
        let Ok(read_dir) = std::fs::read_dir(dir.join(&rel)) else {
            continue;
        };
        for entry in read_dir.flatten() {
            let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
                continue;
            };
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            let child = rel.join(&name);
            if file_type.is_dir() {
                if name.starts_with('.') || SKIPPED.contains(&name.as_str()) {
                    continue;
                }
                if depth >= MAX_DEPTH {
                    too_deep = true;
                    continue;
                }
                stack.push((child, depth + 1));
                continue;
            }
            let is_file = file_type.is_file() || (file_type.is_symlink() && entry.path().is_file());
            if !is_file || !wanted(&child, &name) {
                continue;
            }
            if files.len() >= MAX_FILES {
                too_many = true;
                continue;
            }
            match std::fs::read(entry.path()) {
                Ok(bytes) => files.push(SourceFile::new(child, bytes)),
                Err(err) => warn!(path = %child.display(), %err, "collection file unreadable"),
            }
        }
    }
    if too_deep {
        warn!(dir = %dir.display(), "collection deeper than 16 levels; rest skipped");
    }
    if too_many {
        warn!(dir = %dir.display(), "collection has more than 10000 files; rest skipped");
    }
    let collection = Collection::from_files(dir, files)?;
    for issue in collection.issues() {
        warn!(%issue, "collection issue");
    }
    info!(
        dir = %dir.display(),
        requests = collection.entries().len(),
        folders = folders(collection.root_folder().children()),
        environments = collection.environments().len(),
        issues = collection.issues().len(),
        ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
        "collection loaded"
    );
    Ok(collection)
}

/// The files the model reads, by name and place.
fn wanted(rel: &Path, name: &str) -> bool {
    let top = rel.parent() == Some(Path::new(""));
    let in_envs = rel.parent() == Some(Path::new("envs"));
    (top && (name == "pawsman.toml" || name == ".env"))
        || (in_envs && name.ends_with(".env.toml"))
        || name == "_folder.toml"
        || name.ends_with(".req.toml")
        || name.contains(".body.")
}

fn folders(nodes: &[Node]) -> usize {
    nodes
        .iter()
        .map(|n| match n {
            Node::Folder(f) => 1 + folders(f.children()),
            Node::Entry(_) => 0,
        })
        .sum()
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
            rel = format!("{rel}d{level}/");
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
