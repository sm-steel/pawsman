//! Watching a collection for edits made outside pawsman. Each walked directory is watched on its
//! own, so `.git` and `node_modules` cost nothing; events are debounced into batches of paths.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, RecvTimeoutError, channel};
use std::time::{Duration, Instant};

use notify::event::{AccessKind, AccessMode};
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher as _};
use tracing::{debug, warn};

use crate::CoreError;
use crate::store::relevant;

/// A batch closes after this long without an event.
const QUIET: Duration = Duration::from_millis(100);
/// A batch never stays open longer than this.
const LONGEST: Duration = Duration::from_secs(1);

/// The running watcher; dropping it stops watching.
pub struct Watcher {
    inner: RecommendedWatcher,
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
        let root = root.canonicalize().unwrap_or_else(|_| root.to_owned());
        let (tx, rx) = channel();
        let inner = notify::recommended_watcher(tx).map_err(|e| CoreError::watch(&e))?;
        let prefix = root.clone();
        // The thread ends when `inner` is dropped and closes the channel.
        std::thread::Builder::new()
            .name("pawsman-watch".into())
            .spawn(move || debounce(&prefix, &rx, &sink))
            .map_err(|e| CoreError::watch(&notify::Error::io(e)))?;
        Ok(Self {
            inner,
            root,
            watched: BTreeSet::new(),
        })
    }

    /// Watches each of `dirs` (relative) and stops watching the ones no longer listed.
    pub fn sync(&mut self, dirs: &BTreeSet<PathBuf>) {
        let gone: Vec<PathBuf> = self.watched.difference(dirs).cloned().collect();
        for dir in gone {
            let _ = self.inner.unwatch(&self.root.join(&dir));
            self.watched.remove(&dir);
        }
        for dir in dirs.difference(&self.watched.clone()) {
            match self
                .inner
                .watch(&self.root.join(dir), RecursiveMode::NonRecursive)
            {
                Ok(()) => {
                    self.watched.insert(dir.clone());
                }
                Err(err) => warn!(dir = %dir.display(), %err, "watch failed"),
            }
        }
    }
}

/// Collects events into batches: open on the first event, close after [`QUIET`] without one or
/// [`LONGEST`] in all.
fn debounce(root: &Path, rx: &Receiver<notify::Result<Event>>, sink: &impl Fn(Vec<PathBuf>)) {
    while let Ok(first) = rx.recv() {
        let mut batch = BTreeSet::new();
        collect(root, first, &mut batch);
        let opened = Instant::now();
        let mut open = true;
        while open {
            let left = LONGEST.saturating_sub(opened.elapsed());
            match rx.recv_timeout(QUIET.min(left)) {
                Ok(event) => collect(root, event, &mut batch),
                Err(RecvTimeoutError::Timeout) => open = false,
                Err(RecvTimeoutError::Disconnected) => return,
            }
            open &= !left.is_zero();
        }
        if !batch.is_empty() {
            let paths: Vec<PathBuf> = batch.into_iter().collect();
            debug!(paths = paths.len(), "files changed");
            sink(paths);
        }
    }
}

/// Adds the relevant, relative paths of `event` to `batch`; reads and opens are not changes.
fn collect(root: &Path, event: notify::Result<Event>, batch: &mut BTreeSet<PathBuf>) {
    let event = match event {
        Ok(event) => event,
        Err(err) => {
            warn!(%err, "watch event failed");
            return;
        }
    };
    if let EventKind::Access(kind) = event.kind
        && kind != AccessKind::Close(AccessMode::Write)
    {
        return;
    }
    for path in event.paths {
        let Ok(rel) = path.strip_prefix(root) else {
            continue;
        };
        if !rel.as_os_str().is_empty() && relevant(rel) {
            batch.insert(rel.to_owned());
        }
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
