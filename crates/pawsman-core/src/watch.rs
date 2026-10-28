//! Watching a collection for edits made outside pawsman. Each walked directory is watched on its
//! own, so `.git` and `node_modules` cost nothing; events are debounced into batches of paths.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use crate::CoreError;

/// A batch closes after this long without an event.
const QUIET: std::time::Duration = std::time::Duration::from_millis(100);
/// A batch never stays open longer than this.
const LONGEST: std::time::Duration = std::time::Duration::from_secs(1);

/// The running watcher; dropping it stops watching.
pub struct Watcher {
    root: PathBuf,
    watched: BTreeSet<PathBuf>,
}

impl Watcher {
    /// Starts watching under `root`; `sink` gets each batch of changed paths, relative, sorted and
    /// without the ones the walk skips. Call [`Watcher::sync`] to say which directories to watch.
    ///
    /// # Errors
    ///
    /// `core::watch` when the platform watcher cannot start.
    pub fn start(
        root: &Path,
        sink: impl Fn(Vec<PathBuf>) + Send + 'static,
    ) -> Result<Self, CoreError> {
        let _ = sink;
        Ok(Self {
            root: root.to_owned(),
            watched: BTreeSet::new(),
        })
    }

    /// Watches each of `dirs` (relative) and stops watching the ones no longer listed.
    pub fn sync(&mut self, dirs: &BTreeSet<PathBuf>) {
        let _ = (&self.root, &mut self.watched, dirs);
    }
}

impl std::fmt::Debug for Watcher {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Watcher")
            .field("root", &self.root)
            .field("watched", &self.watched.len())
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc::{Receiver, channel};
    use std::time::{Duration, Instant};

    use super::*;

    fn start(dir: &Path) -> (Watcher, Receiver<Vec<PathBuf>>) {
        let (tx, rx) = channel();
        let mut watcher = Watcher::start(dir, move |batch| {
            let _ = tx.send(batch);
        })
        .unwrap();
        watcher.sync(&dirs(dir));
        (watcher, rx)
    }

    fn dirs(dir: &Path) -> BTreeSet<PathBuf> {
        crate::store::Files::read(dir).dirs().clone()
    }

    fn batch(rx: &Receiver<Vec<PathBuf>>) -> Vec<PathBuf> {
        rx.recv_timeout(Duration::from_secs(2))
            .expect("a batch within 2 s")
    }

    fn paths(list: &[&str]) -> Vec<PathBuf> {
        list.iter().map(PathBuf::from).collect()
    }

    #[test]
    fn a_write_is_reported() {
        let dir = tempfile::tempdir().unwrap();
        let (_watcher, rx) = start(dir.path());

        std::fs::write(dir.path().join("a.req.toml"), "x").unwrap();

        assert_eq!(batch(&rx), paths(&["a.req.toml"]));
    }

    #[test]
    fn a_burst_of_writes_is_one_batch() {
        let dir = tempfile::tempdir().unwrap();
        let (_watcher, rx) = start(dir.path());

        for n in 0..20 {
            std::fs::write(dir.path().join("a.req.toml"), format!("{n}")).unwrap();
            std::thread::sleep(Duration::from_millis(5));
        }

        assert_eq!(batch(&rx), paths(&["a.req.toml"]));
        assert!(
            rx.recv_timeout(Duration::from_millis(300)).is_err(),
            "nothing after the batch"
        );
    }

    #[test]
    fn a_rename_over_the_file_is_reported() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.req.toml"), "old").unwrap();
        let (_watcher, rx) = start(dir.path());

        std::fs::write(dir.path().join(".a.req.toml.tmp"), "new").unwrap();
        std::fs::rename(
            dir.path().join(".a.req.toml.tmp"),
            dir.path().join("a.req.toml"),
        )
        .unwrap();

        assert_eq!(batch(&rx), paths(&["a.req.toml"]), "no hidden temp file");
    }

    #[test]
    fn a_new_subdirectory_is_watched_after_sync() {
        let dir = tempfile::tempdir().unwrap();
        let (mut watcher, rx) = start(dir.path());

        std::fs::create_dir(dir.path().join("users")).unwrap();
        assert_eq!(batch(&rx), paths(&["users"]), "the new directory");
        watcher.sync(&dirs(dir.path()));
        std::fs::write(dir.path().join("users/x.req.toml"), "x").unwrap();

        assert_eq!(batch(&rx), paths(&["users/x.req.toml"]), "inside it");
    }

    #[test]
    fn hidden_paths_are_not_reported() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join(".git")).unwrap();
        let (mut watcher, rx) = start(dir.path());
        let mut all = dirs(dir.path());
        all.insert(PathBuf::from(".git"));
        watcher.sync(&all);
        let started = Instant::now();

        std::fs::write(dir.path().join(".git/HEAD"), "ref").unwrap();
        std::fs::write(dir.path().join(".tmpAbC"), "x").unwrap();
        std::thread::sleep(Duration::from_millis(150));
        std::fs::write(dir.path().join("a.req.toml"), "x").unwrap();

        assert_eq!(batch(&rx), paths(&["a.req.toml"]), "only the real file");
        assert!(started.elapsed() < Duration::from_secs(2), "fast");
    }
}
