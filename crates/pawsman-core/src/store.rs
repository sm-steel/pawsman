//! Collections on disk. The model parses; this walks the directory, caches the bytes, and writes
//! files atomically.

use std::collections::{BTreeMap, BTreeSet};
use std::io::Write as _;
use std::path::{Component, Path, PathBuf};
use std::time::Instant;

use pawsman_model::{Collection, Ids, ModelError, Node, SourceFile, edit};
use tracing::{debug, info, warn};

use crate::CoreError;

/// Folders below this depth are not read.
const MAX_DEPTH: usize = 16;
/// At most this many files are read.
const MAX_FILES: usize = 10_000;
/// Directories never read, besides hidden ones.
const SKIPPED: [&str; 2] = ["node_modules", "target"];
/// Lines `init` puts in `.gitignore`.
const IGNORED: [&str; 2] = [".env", ".pawsman/"];

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
    let collection = Files::read(dir).collection(&mut Ids::default())?;
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

/// The bytes of every collection file, kept between reloads so a change can be compared.
#[derive(Debug, Clone)]
pub struct Files {
    root: PathBuf,
    /// `root` with symlinks resolved, for the symlinked-file check.
    real: PathBuf,
    bytes: BTreeMap<PathBuf, Vec<u8>>,
    dirs: BTreeSet<PathBuf>,
}

/// What a refresh found.
#[derive(Debug, Default)]
#[non_exhaustive]
pub struct Refresh {
    /// Some file's bytes differ from before.
    pub changed: bool,
    /// Files that could not be read; their old bytes stay.
    pub errors: Vec<CoreError>,
}

/// One walk's findings.
#[derive(Default)]
struct Walk {
    files: BTreeMap<PathBuf, Vec<u8>>,
    dirs: BTreeSet<PathBuf>,
    errors: Vec<CoreError>,
}

impl Files {
    /// Walks `dir` with the rules of [`load`].
    #[must_use]
    pub fn read(dir: &Path) -> Self {
        let mut files = Self {
            root: dir.to_owned(),
            real: dir.canonicalize().unwrap_or_else(|_| dir.to_owned()),
            bytes: BTreeMap::new(),
            dirs: BTreeSet::new(),
        };
        let walk = files.walk(Path::new(""), 0);
        for err in &walk.errors {
            warn!(%err, "collection file unreadable");
        }
        files.bytes = walk.files;
        files.dirs = walk.dirs;
        files
    }

    /// The collection directory.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Every directory walked, relative; the root is the empty path.
    #[must_use]
    pub fn dirs(&self) -> &BTreeSet<PathBuf> {
        &self.dirs
    }

    /// The bytes of file `rel`.
    #[must_use]
    pub fn get(&self, rel: &Path) -> Option<&[u8]> {
        self.bytes.get(rel).map(Vec::as_slice)
    }

    /// Every file path, sorted.
    pub fn paths(&self) -> impl Iterator<Item = &Path> {
        self.bytes.keys().map(PathBuf::as_path)
    }

    /// Reads `rel` paths again: a directory is walked anew, a file is read, a missing path is
    /// dropped with everything under it. Paths the walk would skip are ignored.
    pub fn refresh(&mut self, rel: &[PathBuf]) -> Refresh {
        let mut refresh = Refresh::default();
        for path in rel {
            if path.as_os_str().is_empty() || !relevant(path) || !confined(path) {
                continue;
            }
            let full = self.root.join(path);
            match std::fs::symlink_metadata(&full) {
                Ok(meta) if meta.is_dir() => {
                    let old_files: BTreeMap<_, _> = self
                        .bytes
                        .extract_if(.., |p, _| p.starts_with(path))
                        .collect();
                    self.dirs.retain(|d| !d.starts_with(path));
                    let depth = path.components().count();
                    let walk = self.walk(path, depth);
                    refresh.changed |= walk.files != old_files;
                    refresh.errors.extend(walk.errors);
                    self.bytes.extend(walk.files);
                    self.dirs.extend(walk.dirs);
                }
                Ok(_) => {
                    if !self.readable_file(&full) || !wanted(path) {
                        continue;
                    }
                    match std::fs::read(&full) {
                        Ok(bytes) => {
                            refresh.changed |= self.bytes.get(path) != Some(&bytes);
                            self.bytes.insert(path.clone(), bytes);
                        }
                        Err(err) => refresh.errors.push(CoreError::read(path, &err)),
                    }
                }
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                    let before = self.bytes.len();
                    self.bytes.retain(|p, _| !p.starts_with(path));
                    self.dirs.retain(|d| !d.starts_with(path));
                    refresh.changed |= self.bytes.len() != before;
                }
                Err(err) => refresh.errors.push(CoreError::read(path, &err)),
            }
        }
        refresh
    }

    /// Records bytes the engine itself wrote.
    pub fn put(&mut self, rel: &Path, bytes: Vec<u8>) {
        self.bytes.insert(rel.to_owned(), bytes);
    }

    /// Forgets a file the engine itself removed.
    pub fn forget(&mut self, rel: &Path) {
        self.bytes.remove(rel);
    }

    /// Builds the collection, keeping request ids in `ids`.
    ///
    /// # Errors
    ///
    /// As [`Collection::build`].
    pub fn collection(&self, ids: &mut Ids) -> Result<Collection, ModelError> {
        let files = self
            .bytes
            .iter()
            .map(|(p, b)| SourceFile::new(p.clone(), b.clone()))
            .collect();
        Collection::build(&self.root, files, ids)
    }

    /// Walks the directory `start` (relative, `depth` levels below the root), stopping at the
    /// depth and file caps.
    fn walk(&self, start: &Path, depth: usize) -> Walk {
        let mut walk = Walk::default();
        let mut stack = vec![(start.to_owned(), depth)];
        let (mut too_deep, mut too_many) = (false, false);
        let budget = MAX_FILES.saturating_sub(self.bytes.len());
        while let Some((rel, depth)) = stack.pop() {
            let Ok(read_dir) = std::fs::read_dir(self.root.join(&rel)) else {
                continue;
            };
            walk.dirs.insert(rel.clone());
            for entry in read_dir.flatten() {
                let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
                    continue;
                };
                let Ok(file_type) = entry.file_type() else {
                    continue;
                };
                let child = rel.join(&name);
                if !relevant(&child) {
                    continue;
                }
                if file_type.is_dir() && depth >= MAX_DEPTH {
                    too_deep = true;
                    continue;
                }
                if file_type.is_dir() {
                    stack.push((child, depth + 1));
                    continue;
                }
                let is_file = file_type.is_file()
                    || (file_type.is_symlink() && self.readable_file(&entry.path()));
                if !is_file || !wanted(&child) {
                    continue;
                }
                if walk.files.len() >= budget {
                    too_many = true;
                    continue;
                }
                match std::fs::read(entry.path()) {
                    Ok(bytes) => {
                        walk.files.insert(child, bytes);
                    }
                    Err(err) => walk.errors.push(CoreError::read(&child, &err)),
                }
            }
        }
        if too_deep {
            warn!(dir = %self.root.display(), "collection deeper than 16 levels; rest skipped");
        }
        if too_many {
            warn!(dir = %self.root.display(), "collection has more than 10000 files; rest skipped");
        }
        walk
    }

    /// A regular file, or a symlink to one inside the collection.
    fn readable_file(&self, path: &Path) -> bool {
        match std::fs::symlink_metadata(path) {
            Ok(meta) if meta.file_type().is_symlink() => path.is_file() && inside(&self.real, path),
            Ok(meta) => meta.is_file(),
            Err(_) => false,
        }
    }
}

/// Whether `rel` can hold collection files: no hidden component (but the top-level `.env`), no
/// `node_modules` or `target`.
#[must_use]
pub fn relevant(rel: &Path) -> bool {
    if rel == Path::new(".env") {
        return true;
    }
    rel.components().all(|c| match c {
        Component::Normal(name) => name
            .to_str()
            .is_some_and(|n| !n.starts_with('.') && !SKIPPED.contains(&n)),
        _ => false,
    })
}

/// Relative, with no `..`, root or prefix: it stays under the collection.
fn confined(rel: &Path) -> bool {
    rel.components().all(|c| matches!(c, Component::Normal(_)))
}

/// Writes `bytes` to `rel` under `root` atomically: a temp file in the same directory, synced, then
/// renamed over the target. The target's permissions are kept; a new file gets `0644` on Unix.
///
/// # Errors
///
/// `core::write` when `rel` leaves the collection, or the directory or file cannot be written.
pub fn write(root: &Path, rel: &Path, bytes: &[u8]) -> Result<(), CoreError> {
    if !confined(rel) || rel.as_os_str().is_empty() {
        return Err(CoreError::write(
            rel,
            &std::io::Error::other("the path leaves the collection"),
        ));
    }
    let target = root.join(rel);
    let fail = |err: std::io::Error| CoreError::write(rel, &err);
    let parent = target.parent().unwrap_or(root);
    std::fs::create_dir_all(parent).map_err(fail)?;
    let mut temp = tempfile::NamedTempFile::new_in(parent).map_err(fail)?;
    temp.write_all(bytes).map_err(fail)?;
    match std::fs::metadata(&target) {
        Ok(meta) => temp.as_file().set_permissions(meta.permissions()),
        Err(_) => set_new_file_mode(temp.as_file()),
    }
    .map_err(fail)?;
    temp.as_file().sync_all().map_err(fail)?;
    temp.persist(&target).map_err(|e| fail(e.error))?;
    debug!(path = %rel.display(), bytes = bytes.len(), "written");
    Ok(())
}

/// `0644` for a new file; the temp file starts as `0600`.
#[cfg(unix)]
fn set_new_file_mode(file: &std::fs::File) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt as _;
    file.set_permissions(std::fs::Permissions::from_mode(0o644))
}

#[cfg(not(unix))]
fn set_new_file_mode(_file: &std::fs::File) -> std::io::Result<()> {
    Ok(())
}

/// Creates a collection in `dir`: `pawsman.toml` named after the directory, and `.gitignore` lines
/// for `.env` and `.pawsman/`.
///
/// # Errors
///
/// `core::exists` when `dir` already holds `pawsman.toml`; `core::write` when a file cannot be
/// written.
pub fn init(dir: &Path) -> Result<(), CoreError> {
    let manifest = dir.join("pawsman.toml");
    if manifest.exists() {
        return Err(CoreError::exists(&manifest));
    }
    std::fs::create_dir_all(dir).map_err(|e| CoreError::write(dir, &e))?;
    let name = dir
        .canonicalize()
        .ok()
        .and_then(|d| d.file_name().map(|n| n.to_string_lossy().into_owned()))
        .unwrap_or_else(|| "collection".to_owned());
    write(
        dir,
        Path::new("pawsman.toml"),
        edit::new_manifest(&name).as_bytes(),
    )?;
    let ignore = dir.join(".gitignore");
    let mut text = match std::fs::read_to_string(&ignore) {
        Ok(text) => text,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(err) => return Err(CoreError::write(&ignore, &err)),
    };
    let missing: Vec<&str> = IGNORED
        .into_iter()
        .filter(|line| !text.lines().any(|l| l.trim() == *line))
        .collect();
    if !missing.is_empty() {
        if !text.is_empty() && !text.ends_with('\n') {
            text.push('\n');
        }
        for line in missing {
            text.push_str(line);
            text.push('\n');
        }
        write(dir, Path::new(".gitignore"), text.as_bytes())?;
    }
    info!(dir = %dir.display(), "collection created");
    Ok(())
}

/// Whether `path`, symlinks followed, lies under `root`; a link out of the collection is not read.
fn inside(root: &Path, path: &Path) -> bool {
    path.canonicalize().is_ok_and(|real| real.starts_with(root))
}

/// The files the model reads, by name and place.
fn wanted(rel: &Path) -> bool {
    let Some(name) = rel.file_name().and_then(|n| n.to_str()) else {
        return false;
    };
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

    fn file(dir: &Path, name: &str, text: &str) {
        std::fs::write(dir.join(name), text).unwrap();
    }

    #[test]
    fn loads_requests_and_ignores_other_files() {
        let dir = tempfile::tempdir().unwrap();
        file(dir.path(), "pawsman.toml", "format = 1\nname = \"D\"\n");
        file(
            dir.path(),
            "a.req.toml",
            "name = \"A\"\nurl = \"http://h\"\n",
        );
        file(dir.path(), "notes.md", "# not a request");
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

    #[cfg(unix)]
    #[test]
    fn a_symlinked_file_pointing_outside_is_not_read() {
        let outside = tempfile::tempdir().unwrap();
        std::fs::write(outside.path().join("key"), "PRIVATE").unwrap();
        let dir = tempfile::tempdir().unwrap();
        put(dir.path(), "pawsman.toml", MANIFEST);
        put(
            dir.path(),
            "a.req.toml",
            "name = \"a\"\nurl = \"u\"\n[body]\ntext = { sidecar = \"a.body.txt\" }\n",
        );
        std::os::unix::fs::symlink(outside.path().join("key"), dir.path().join("a.body.txt"))
            .unwrap();

        let c = load(dir.path()).unwrap();

        assert!(
            matches!(c.entries()[0].kind(), EntryKind::Invalid(e) if e.code() == pawsman_model::codes::MISSING_SIDECAR),
            "{:?}",
            c.entries()[0].kind()
        );
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
        file(
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

    fn refreshed(files: &mut Files, rel: &str) -> Refresh {
        files.refresh(&[PathBuf::from(rel)])
    }

    #[test]
    fn refresh_reads_a_changed_file() {
        let dir = tempfile::tempdir().unwrap();
        put(dir.path(), "pawsman.toml", MANIFEST);
        put(dir.path(), "a.req.toml", REQ);
        let mut files = Files::read(dir.path());

        put(dir.path(), "a.req.toml", "name = \"new\"\nurl = \"u\"\n");
        let refresh = refreshed(&mut files, "a.req.toml");

        assert!(refresh.changed && refresh.errors.is_empty(), "{refresh:?}");
        assert_eq!(
            files.get(Path::new("a.req.toml")),
            Some(&b"name = \"new\"\nurl = \"u\"\n"[..]),
            "new bytes"
        );
    }

    #[test]
    fn refresh_drops_a_removed_file() {
        let dir = tempfile::tempdir().unwrap();
        put(dir.path(), "pawsman.toml", MANIFEST);
        put(dir.path(), "a.req.toml", REQ);
        let mut files = Files::read(dir.path());
        assert!(
            files.get(Path::new("a.req.toml")).is_some(),
            "read at first"
        );

        std::fs::remove_file(dir.path().join("a.req.toml")).unwrap();
        let refresh = refreshed(&mut files, "a.req.toml");

        assert!(refresh.changed, "changed");
        assert!(files.get(Path::new("a.req.toml")).is_none(), "gone");
    }

    #[test]
    fn refresh_walks_a_new_directory_and_drops_a_removed_one() {
        let dir = tempfile::tempdir().unwrap();
        put(dir.path(), "pawsman.toml", MANIFEST);
        put(dir.path(), "old/x.req.toml", REQ);
        let mut files = Files::read(dir.path());

        put(dir.path(), "users/sub/a.req.toml", REQ);
        std::fs::remove_dir_all(dir.path().join("old")).unwrap();
        let refresh = files.refresh(&[PathBuf::from("users"), PathBuf::from("old")]);

        assert!(refresh.changed, "changed");
        assert_eq!(
            files.paths().collect::<Vec<_>>(),
            [Path::new("pawsman.toml"), Path::new("users/sub/a.req.toml")],
            "walked and dropped"
        );
        assert!(
            files.dirs().contains(Path::new("users/sub"))
                && !files.dirs().contains(Path::new("old")),
            "{:?}",
            files.dirs()
        );
    }

    #[test]
    fn refresh_with_the_same_bytes_is_unchanged() {
        let dir = tempfile::tempdir().unwrap();
        put(dir.path(), "pawsman.toml", MANIFEST);
        put(dir.path(), "u/a.req.toml", REQ);
        let mut files = Files::read(dir.path());

        put(dir.path(), "u/a.req.toml", REQ);
        let refresh = files.refresh(&[PathBuf::from("u/a.req.toml"), PathBuf::from("u")]);

        assert!(!refresh.changed, "same bytes");
    }

    #[test]
    fn files_read_lists_the_root_and_walked_dirs() {
        let dir = tempfile::tempdir().unwrap();
        put(dir.path(), "pawsman.toml", MANIFEST);
        put(dir.path(), "users/a.req.toml", REQ);
        put(dir.path(), ".git/HEAD", "ref");

        let files = Files::read(dir.path());

        assert_eq!(
            files
                .dirs()
                .iter()
                .map(PathBuf::as_path)
                .collect::<Vec<_>>(),
            [Path::new(""), Path::new("users")],
            "no hidden dirs"
        );
    }

    #[rstest::rstest]
    #[case(".git/x.req.toml", false)]
    #[case(".tmpAbC", false)]
    #[case("a/.x.swp", false)]
    #[case("node_modules/a.req.toml", false)]
    #[case("a/target", false)]
    #[case("a/.env", false)]
    #[case(".env", true)]
    #[case("users/a.req.toml", true)]
    #[case("users", true)]
    fn hidden_and_skipped_paths_are_not_relevant(#[case] rel: &str, #[case] expected: bool) {
        assert_eq!(relevant(Path::new(rel)), expected, "{rel}");
    }

    #[cfg(unix)]
    fn mode(path: &Path) -> u32 {
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::metadata(path).unwrap().permissions().mode() & 0o777
    }

    #[cfg(unix)]
    #[test]
    fn write_replaces_atomically_and_keeps_permissions() {
        use std::os::unix::fs::PermissionsExt as _;
        let dir = tempfile::tempdir().unwrap();
        put(dir.path(), "a.req.toml", "old");
        std::fs::set_permissions(
            dir.path().join("a.req.toml"),
            std::fs::Permissions::from_mode(0o640),
        )
        .unwrap();

        write(dir.path(), Path::new("a.req.toml"), b"new").unwrap();

        assert_eq!(
            std::fs::read(dir.path().join("a.req.toml")).unwrap(),
            b"new",
            "content"
        );
        assert_eq!(mode(&dir.path().join("a.req.toml")), 0o640, "mode kept");
        assert_eq!(
            std::fs::read_dir(dir.path()).unwrap().count(),
            1,
            "no temp file left"
        );
    }

    #[cfg(unix)]
    #[test]
    fn write_creates_a_new_file_0644_and_its_folder() {
        let dir = tempfile::tempdir().unwrap();

        write(
            dir.path(),
            Path::new("users/_folder.toml"),
            b"name = \"U\"\n",
        )
        .unwrap();

        assert_eq!(mode(&dir.path().join("users/_folder.toml")), 0o644, "mode");
    }

    #[test]
    fn write_refuses_paths_outside_the_collection() {
        let dir = tempfile::tempdir().unwrap();

        for rel in ["../x", "/tmp/x", "a/../../x"] {
            let err = write(dir.path(), Path::new(rel), b"x").unwrap_err();
            assert_eq!(err.code(), crate::codes::WRITE, "{rel}");
        }
    }

    #[cfg(unix)]
    #[test]
    fn write_into_a_read_only_dir_is_core_write() {
        use std::os::unix::fs::PermissionsExt as _;
        let dir = tempfile::tempdir().unwrap();
        std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o500)).unwrap();
        if std::fs::write(dir.path().join("probe"), b"").is_ok() {
            return; // root ignores permissions
        }

        let err = write(dir.path(), Path::new("a.req.toml"), b"x").unwrap_err();
        std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700)).unwrap();

        assert_eq!(err.code(), crate::codes::WRITE, "code");
        assert!(err.to_string().contains("a.req.toml"), "{err}");
    }

    #[test]
    fn init_writes_manifest_and_gitignore() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("my api");

        init(&target).unwrap();

        let c = load(&target).unwrap();
        assert_eq!(c.name(), "my api", "named after the directory");
        assert_eq!(
            std::fs::read_to_string(target.join(".gitignore")).unwrap(),
            ".env\n.pawsman/\n",
            "gitignore"
        );
    }

    #[test]
    fn init_refuses_an_existing_collection() {
        let dir = tempfile::tempdir().unwrap();
        put(dir.path(), "pawsman.toml", MANIFEST);

        let err = init(dir.path()).unwrap_err();

        assert_eq!(err.code(), crate::codes::EXISTS, "code");
        assert_eq!(
            std::fs::read_to_string(dir.path().join("pawsman.toml")).unwrap(),
            MANIFEST,
            "untouched"
        );
    }

    #[test]
    fn init_appends_to_an_existing_gitignore_once() {
        let dir = tempfile::tempdir().unwrap();
        put(dir.path(), ".gitignore", "target\n.env");

        init(dir.path()).unwrap();

        assert_eq!(
            std::fs::read_to_string(dir.path().join(".gitignore")).unwrap(),
            "target\n.env\n.pawsman/\n",
            "only the missing line, after a newline"
        );
    }
}
