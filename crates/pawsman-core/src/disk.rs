//! The engine's side of the collection on disk: the cached files, stable ids, the watcher, and
//! running tree operations so that their own writes never reload twice.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::time::Instant;

use pawsman_model::{Collection, Ids, ModelError};
use tracing::{info, warn};

use crate::notice::Notice;
use crate::store::{self, Files};
use crate::tree::{self, Change, NodeRef, Op};
use crate::watch::Watcher;

/// The collection directory as the engine holds it.
pub(crate) struct Disk {
    files: Files,
    ids: Ids,
    watcher: Option<Watcher>,
}

/// What an operation did.
pub(crate) struct Done {
    /// The collection rebuilt from the new files.
    pub(crate) collection: Result<Collection, ModelError>,
    /// The node to select.
    pub(crate) select: Option<PathBuf>,
    /// Notices: kept folders, a failed change.
    pub(crate) notices: Vec<Notice>,
}

impl Disk {
    /// Reads `dir`, builds the collection, and starts watching when it is one; `changed` gets the
    /// watcher's batches.
    pub(crate) fn open(
        dir: &Path,
        changed: impl Fn(Vec<PathBuf>) + Send + 'static,
    ) -> (Self, Result<Collection, ModelError>, Vec<Notice>) {
        let started = Instant::now();
        let files = Files::read(dir);
        let mut ids = Ids::default();
        let collection = files.collection(&mut ids);
        let mut notices = Vec::new();
        let mut disk = Self {
            files,
            ids,
            watcher: None,
        };
        if let Ok(collection) = &collection {
            store::log_loaded(dir, collection, started);
            match Watcher::start(dir, changed) {
                Ok(watcher) => disk.watcher = Some(watcher),
                Err(err) => {
                    warn!(%err, "watcher did not start");
                    notices.push(Notice::from_error(&err));
                }
            }
            disk.sync();
        }
        (disk, collection, notices)
    }

    /// The collection directory.
    pub(crate) fn root(&self) -> &Path {
        self.files.root()
    }

    /// Reads the changed paths again; the rebuilt collection when a byte changed.
    pub(crate) fn changed(
        &mut self,
        paths: &[PathBuf],
    ) -> (Option<Result<Collection, ModelError>>, Vec<Notice>) {
        let started = Instant::now();
        let refresh = self.files.refresh(paths);
        let notices = refresh
            .errors
            .iter()
            .inspect(|err| warn!(%err, "collection file unreadable"))
            .map(|err| Notice::from_error(err))
            .collect();
        self.sync();
        if !refresh.changed {
            return (None, notices);
        }
        let collection = self.rebuild();
        info!(
            paths = paths.len(),
            ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
            "collection reloaded"
        );
        (Some(collection), notices)
    }

    /// Reads the whole collection again.
    pub(crate) fn reload(&mut self) -> Result<Collection, ModelError> {
        self.files = Files::read(self.files.root());
        self.sync();
        info!("collection reloaded by hand");
        self.rebuild()
    }

    /// Plans and runs `op`; `None` when it changes nothing.
    pub(crate) fn run(&mut self, collection: &Collection, op: &Op) -> Option<Done> {
        // An edit the watcher has not reported yet must not be overwritten from a stale cache.
        let reads = reads(collection, op);
        let fresh = self.files.refresh(&reads).changed.then(|| self.rebuild());
        let collection = match &fresh {
            Some(Ok(fresh)) => fresh,
            _ => collection,
        };
        let plan = match tree::plan(collection, &self.files, op) {
            Ok(plan) if plan.changes.is_empty() => return None,
            Ok(plan) => plan,
            Err(notice) => {
                return Some(Done {
                    collection: self.rebuild(),
                    select: None,
                    notices: vec![notice],
                });
            }
        };
        let mut notices = Vec::new();
        match store::apply(self.files.root(), &plan.changes) {
            Ok(kept) => {
                for change in &plan.changes {
                    if let Change::Rename { from, to } = change {
                        self.ids.rename(from, to);
                    }
                }
                notices.extend(kept.iter().map(|dir| {
                    Notice::info(format!("`{}` kept: it holds other files", dir.display()))
                }));
            }
            Err(err) => {
                warn!(%err, "tree change failed");
                notices.push(Notice::from_error(&err));
            }
        }
        info!(op = kind(op), changes = plan.changes.len(), "tree changed");
        let touched = self.touched(&plan.changes);
        let refresh = self.files.refresh(&touched);
        notices.extend(refresh.errors.iter().map(|e| Notice::from_error(e)));
        self.sync();
        Some(Done {
            collection: self.rebuild(),
            select: plan.select,
            notices,
        })
    }

    /// The paths a plan touched, with each new directory, so the cache sees them at once.
    fn touched(&self, changes: &[Change]) -> Vec<PathBuf> {
        let mut paths = BTreeSet::new();
        for change in changes {
            let path = match change {
                Change::Write { path, .. }
                | Change::Remove(path)
                | Change::RemoveDirIfEmpty(path) => path,
                Change::Rename { from, to } => {
                    paths.insert(from.clone());
                    to
                }
            };
            paths.insert(path.clone());
            for dir in path.ancestors().skip(1) {
                if dir.as_os_str().is_empty() || self.files.dirs().contains(dir) {
                    break;
                }
                paths.insert(dir.to_owned());
            }
        }
        paths.into_iter().collect()
    }

    fn rebuild(&mut self) -> Result<Collection, ModelError> {
        let collection = self.files.collection(&mut self.ids);
        let files = &self.files;
        self.ids.retain(|p| files.get(p).is_some());
        collection
    }

    fn sync(&mut self) {
        if let Some(watcher) = &mut self.watcher {
            watcher.sync(self.files.dirs());
        }
    }
}

/// The files a plan for `op` reads: the node's own file and the `_folder.toml` files whose
/// `name` or `order` it may rewrite.
fn reads(collection: &Collection, op: &Op) -> Vec<PathBuf> {
    let node = match op {
        Op::Rename { node, .. } | Op::Delete(node) | Op::Move { node, .. } => node,
        Op::NewRequest { .. } | Op::NewFolder { .. } => return Vec::new(),
    };
    match node {
        NodeRef::Request(id) => collection
            .entry(*id)
            .map(|e| vec![e.path().to_owned(), e.folder().join(FOLDER_FILE)])
            .unwrap_or_default(),
        NodeRef::Folder(dir) => vec![
            dir.join(FOLDER_FILE),
            dir.parent()
                .map_or_else(|| PathBuf::from(FOLDER_FILE), |p| p.join(FOLDER_FILE)),
        ],
    }
}

const FOLDER_FILE: &str = "_folder.toml";

/// A short name of the operation for the log; names and contents stay out of it.
fn kind(op: &Op) -> &'static str {
    match op {
        Op::NewRequest { .. } => "new request",
        Op::NewFolder { .. } => "new folder",
        Op::Rename { .. } => "rename",
        Op::Delete(_) => "delete",
        Op::Move { .. } => "move",
    }
}
