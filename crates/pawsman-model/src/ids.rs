//! Request ids that stay the same while a collection is rebuilt, keyed by file path.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::RequestId;

/// Which id each request file has; ids are never reused within a session.
#[derive(Debug, Clone, Default)]
pub struct Ids {
    by_path: BTreeMap<PathBuf, RequestId>,
    last: u32,
}

impl Ids {
    /// The id of `path`, or the next unused one.
    pub fn id(&mut self, path: &Path) -> RequestId {
        if let Some(id) = self.by_path.get(path) {
            return *id;
        }
        self.last = self.last.saturating_add(1);
        let id = RequestId::new(self.last);
        self.by_path.insert(path.to_owned(), id);
        id
    }

    /// Moves `from`, and every path under it, to the same place under `to`.
    pub fn rename(&mut self, from: &Path, to: &Path) {
        let moved: Vec<PathBuf> = self
            .by_path
            .keys()
            .filter(|p| p.starts_with(from))
            .cloned()
            .collect();
        for old in moved {
            let Some(id) = self.by_path.remove(&old) else {
                continue;
            };
            let new = match old.strip_prefix(from) {
                Ok(rest) if !rest.as_os_str().is_empty() => to.join(rest),
                _ => to.to_owned(),
            };
            self.by_path.insert(new, id);
        }
    }

    /// Forgets the paths for which `live` is false.
    pub fn retain(&mut self, live: impl Fn(&Path) -> bool) {
        self.by_path.retain(|path, _| live(path));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_paths_keep_their_id_and_new_ones_count_up() {
        let mut ids = Ids::default();

        let a = ids.id(Path::new("a.req.toml"));
        let b = ids.id(Path::new("b.req.toml"));

        assert_eq!((a, b), (RequestId::new(1), RequestId::new(2)), "in order");
        assert_eq!(ids.id(Path::new("a.req.toml")), a, "same path, same id");
    }

    #[test]
    fn rename_moves_a_file_and_everything_under_a_folder() {
        let mut ids = Ids::default();
        let x = ids.id(Path::new("users/x.req.toml"));
        let y = ids.id(Path::new("users/sub/y.req.toml"));
        let other = ids.id(Path::new("usersx/z.req.toml"));

        ids.rename(Path::new("users"), Path::new("people"));

        assert_eq!(ids.id(Path::new("people/x.req.toml")), x, "file below");
        assert_eq!(ids.id(Path::new("people/sub/y.req.toml")), y, "deeper");
        assert_eq!(
            ids.id(Path::new("usersx/z.req.toml")),
            other,
            "a sibling with the same prefix stays"
        );
    }

    #[test]
    fn retain_forgets_removed_paths_and_never_reuses_numbers() {
        let mut ids = Ids::default();
        ids.id(Path::new("a.req.toml"));
        let b = ids.id(Path::new("b.req.toml"));

        ids.retain(|p| p != Path::new("a.req.toml"));

        assert_eq!(ids.id(Path::new("b.req.toml")), b, "kept");
        assert_eq!(
            ids.id(Path::new("a.req.toml")),
            RequestId::new(3),
            "a new number for a returning path"
        );
    }
}
