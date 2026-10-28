//! Tree operations: create, rename, delete and reorder requests and folders. `plan` is pure: it
//! turns an operation into file changes from the cached bytes; `store::apply` runs them.

use std::path::{Path, PathBuf};

use pawsman_model::{Collection, Folder, ModelError, Node, RequestId, edit};

use crate::CoreError;
use crate::notice::Notice;
use crate::store::Files;

/// A node of the tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NodeRef {
    /// A request file.
    Request(RequestId),
    /// A folder, by its path relative to the root; the root is the empty path.
    Folder(PathBuf),
}

/// Which way a node moves among its siblings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    /// Towards the top.
    Up,
    /// Towards the bottom.
    Down,
}

/// What to do to the tree.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Op {
    /// A new GET request named `name` in folder `parent`.
    NewRequest {
        /// The folder, relative; the root is empty.
        parent: PathBuf,
        /// The display name.
        name: String,
    },
    /// A new folder named `name` in folder `parent`.
    NewFolder {
        /// The folder, relative; the root is empty.
        parent: PathBuf,
        /// The display name.
        name: String,
    },
    /// Rename a node: its `name` and its file or directory.
    Rename {
        /// The node.
        node: NodeRef,
        /// The new display name.
        name: String,
    },
    /// Delete a node and the collection files under it.
    Delete(NodeRef),
    /// Swap a node with its neighbour.
    Move {
        /// The node.
        node: NodeRef,
        /// Which neighbour.
        dir: Direction,
    },
}

/// One file system change, with paths relative to the collection root.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Change {
    /// Write a file atomically.
    Write {
        /// The file.
        path: PathBuf,
        /// Its new bytes.
        bytes: Vec<u8>,
    },
    /// Rename a file or directory; the target must not exist.
    Rename {
        /// The old path.
        from: PathBuf,
        /// The new path.
        to: PathBuf,
    },
    /// Remove a file.
    Remove(PathBuf),
    /// Remove a directory if nothing is left in it.
    RemoveDirIfEmpty(PathBuf),
}

/// The changes for one operation and the node to select afterwards.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Plan {
    /// The changes, in order.
    pub changes: Vec<Change>,
    /// The path of the new or renamed node: a request file or a folder directory.
    pub select: Option<PathBuf>,
}

/// The changes `op` needs.
///
/// # Errors
///
/// A notice for `core::exists` when a rename target is taken, or `model::invalid_file` when a
/// file to change does not parse.
pub fn plan(collection: &Collection, files: &Files, op: &Op) -> Result<Plan, Notice> {
    let planner = Planner { collection, files };
    match op {
        Op::NewRequest { parent, name } => Ok(planner.new_request(parent, name)),
        Op::NewFolder { parent, name } => Ok(planner.new_folder(parent, name)),
        Op::Rename { node, name } => planner.rename(node, name),
        Op::Delete(node) => planner.delete(node),
        Op::Move { node, dir } => planner.move_node(node, *dir),
    }
}

const FOLDER_FILE: &str = "_folder.toml";
const REQUEST_EXT: &str = ".req.toml";
/// Names the walk skips or the layout reserves, so a new node never gets them.
const RESERVED_AT_ROOT: [&str; 1] = ["envs"];
const RESERVED: [&str; 2] = ["node_modules", "target"];

struct Planner<'a> {
    collection: &'a Collection,
    files: &'a Files,
}

/// Where a node sits: its parent folder and its key in the parent's `order`.
struct Place {
    parent: PathBuf,
    key: String,
}

impl Planner<'_> {
    fn new_request(&self, parent: &Path, name: &str) -> Plan {
        if self.folder(parent).is_none() {
            return Plan::default();
        }
        let key = self.free_key(parent, name, "request");
        let path = parent.join(format!("{key}{REQUEST_EXT}"));
        Plan {
            changes: vec![Change::Write {
                path: path.clone(),
                bytes: edit::new_request(name).into_bytes(),
            }],
            select: Some(path),
        }
    }

    fn new_folder(&self, parent: &Path, name: &str) -> Plan {
        if self.folder(parent).is_none() {
            return Plan::default();
        }
        let dir = parent.join(self.free_key(parent, name, "folder"));
        Plan {
            changes: vec![Change::Write {
                path: dir.join(FOLDER_FILE),
                bytes: edit::new_folder(name).into_bytes(),
            }],
            select: Some(dir),
        }
    }

    fn rename(&self, node: &NodeRef, name: &str) -> Result<Plan, Notice> {
        let Some(place) = self.place(node) else {
            return Ok(Plan::default());
        };
        let wanted = match edit::slug(name) {
            slug if slug.is_empty() => place.key.clone(),
            slug => slug,
        };
        let mut changes = Vec::new();
        let (old, new, named_file) = match node {
            NodeRef::Request(_) => {
                let old = place.parent.join(format!("{}{REQUEST_EXT}", place.key));
                let new = place.parent.join(format!("{wanted}{REQUEST_EXT}"));
                (old.clone(), new.clone(), (old, new))
            }
            NodeRef::Folder(dir) => {
                let new = place.parent.join(&wanted);
                (
                    dir.clone(),
                    new.clone(),
                    (dir.join(FOLDER_FILE), new.join(FOLDER_FILE)),
                )
            }
        };
        if wanted != place.key {
            if self.taken(&place.parent, &wanted) {
                return Err(Notice::from_error(&CoreError::exists(&new)));
            }
            changes.push(Change::Rename {
                from: old,
                to: new.clone(),
            });
        }
        let (text_at, write_at) = named_file;
        let text = self.text(&text_at)?;
        let renamed = edit::set_name(&text, &text_at, name).map_err(|e| Notice::from_error(&e))?;
        changes.push(Change::Write {
            path: write_at,
            bytes: renamed.into_bytes(),
        });
        if wanted != place.key {
            let order_at = place.parent.join(FOLDER_FILE);
            let text = self.text(&order_at)?;
            if let Some(order) = edit::rename_in_order(&text, &order_at, &place.key, &wanted)
                .map_err(|e| Notice::from_error(&e))?
            {
                changes.push(Change::Write {
                    path: order_at,
                    bytes: order.into_bytes(),
                });
            }
        }
        Ok(Plan {
            changes,
            select: Some(new),
        })
    }

    fn delete(&self, node: &NodeRef) -> Result<Plan, Notice> {
        let Some(place) = self.place(node) else {
            return Ok(Plan::default());
        };
        let mut changes = Vec::new();
        match node {
            NodeRef::Request(_) => {
                changes.push(Change::Remove(
                    place.parent.join(format!("{}{REQUEST_EXT}", place.key)),
                ));
                let sidecar = format!("{}.body.", place.key);
                changes.extend(
                    self.files
                        .paths()
                        .filter(|p| {
                            p.parent() == Some(place.parent.as_path())
                                && p.file_name()
                                    .and_then(|n| n.to_str())
                                    .is_some_and(|n| n.starts_with(&sidecar))
                        })
                        .map(|p| Change::Remove(p.to_owned())),
                );
            }
            NodeRef::Folder(dir) => {
                changes.extend(
                    self.files
                        .paths()
                        .filter(|p| p.starts_with(dir))
                        .map(|p| Change::Remove(p.to_owned())),
                );
                let mut dirs: Vec<&PathBuf> = self
                    .files
                    .dirs()
                    .iter()
                    .filter(|d| d.starts_with(dir))
                    .collect();
                dirs.sort_by_key(|d| std::cmp::Reverse(d.components().count()));
                changes.extend(
                    dirs.into_iter()
                        .map(|d| Change::RemoveDirIfEmpty(d.clone())),
                );
            }
        }
        let order_at = place.parent.join(FOLDER_FILE);
        let text = self.text(&order_at)?;
        if let Some(order) = edit::remove_from_order(&text, &order_at, &place.key)
            .map_err(|e| Notice::from_error(&e))?
        {
            changes.push(Change::Write {
                path: order_at,
                bytes: order.into_bytes(),
            });
        }
        Ok(Plan {
            changes,
            select: None,
        })
    }

    fn move_node(&self, node: &NodeRef, dir: Direction) -> Result<Plan, Notice> {
        let Some(place) = self.place(node) else {
            return Ok(Plan::default());
        };
        let Some(parent) = self.folder(&place.parent) else {
            return Ok(Plan::default());
        };
        let mut keys = self.keys(parent);
        let Some(at) = keys.iter().position(|k| *k == place.key) else {
            return Ok(Plan::default());
        };
        let other = match dir {
            Direction::Up => at.checked_sub(1),
            Direction::Down => Some(at + 1).filter(|i| *i < keys.len()),
        };
        let Some(other) = other else {
            return Ok(Plan::default());
        };
        keys.swap(at, other);
        let order_at = place.parent.join(FOLDER_FILE);
        let text = self.text(&order_at)?;
        let order = edit::set_order(&text, &order_at, &keys).map_err(|e| Notice::from_error(&e))?;
        Ok(Plan {
            changes: vec![Change::Write {
                path: order_at,
                bytes: order.into_bytes(),
            }],
            select: None,
        })
    }

    /// The folder at `path`; the root is the empty path.
    fn folder(&self, path: &Path) -> Option<&Folder> {
        let mut current = self.collection.root_folder();
        for part in path.components() {
            let prefix = current.path().join(part);
            current = current.children().iter().find_map(|n| match n {
                Node::Folder(f) if f.path() == prefix => Some(&**f),
                _ => None,
            })?;
        }
        Some(current)
    }

    /// The parent and key of a node; `None` for the root or an unknown node.
    fn place(&self, node: &NodeRef) -> Option<Place> {
        match node {
            NodeRef::Request(id) => {
                let entry = self.collection.entry(*id)?;
                Some(Place {
                    parent: entry.folder().to_owned(),
                    key: entry.stem().to_owned(),
                })
            }
            NodeRef::Folder(dir) => {
                self.folder(dir)?;
                let key = dir.file_name()?.to_str()?.to_owned();
                Some(Place {
                    parent: dir.parent().map(Path::to_owned).unwrap_or_default(),
                    key,
                })
            }
        }
    }

    /// The `order` keys of a folder's children, in display order.
    fn keys(&self, folder: &Folder) -> Vec<String> {
        folder
            .children()
            .iter()
            .filter_map(|n| match n {
                Node::Folder(f) => f.path().file_name()?.to_str().map(str::to_owned),
                Node::Entry(id) => self.collection.entry(*id).map(|e| e.stem().to_owned()),
            })
            .collect()
    }

    /// Whether `key` names a request, a directory or a reserved name in `parent`.
    fn taken(&self, parent: &Path, key: &str) -> bool {
        let reserved = RESERVED.contains(&key)
            || (parent.as_os_str().is_empty() && RESERVED_AT_ROOT.contains(&key));
        reserved
            || self
                .files
                .get(&parent.join(format!("{key}{REQUEST_EXT}")))
                .is_some()
            || self.files.dirs().contains(&parent.join(key))
    }

    /// The slug of `name` (or `fallback`), with `-2`, `-3`, … until it is free in `parent`.
    fn free_key(&self, parent: &Path, name: &str, fallback: &str) -> String {
        let base = match edit::slug(name) {
            slug if slug.is_empty() => fallback.to_owned(),
            slug => slug,
        };
        let mut key = base.clone();
        let mut n = 1u32;
        while self.taken(parent, &key) {
            n += 1;
            key = format!("{base}-{n}");
        }
        key
    }

    /// The cached text of `path`; empty when the file does not exist.
    fn text(&self, path: &Path) -> Result<String, Notice> {
        match self.files.get(path) {
            None => Ok(String::new()),
            Some(bytes) => String::from_utf8(bytes.to_vec())
                .map_err(|_| Notice::from_error(&ModelError::invalid_file(path, "not UTF-8"))),
        }
    }
}

#[cfg(test)]
mod tests {
    use pawsman_model::Ids;

    use super::*;

    const MANIFEST: &str = "format = 1\nname = \"C\"\n";

    fn req(name: &str) -> String {
        format!("name = \"{name}\"\nurl = \"http://h\"\n")
    }

    fn setup(pairs: &[(&str, &str)]) -> (Collection, Files) {
        let mut all = vec![("pawsman.toml", MANIFEST)];
        all.extend_from_slice(pairs);
        let files = Files::from_pairs(Path::new("/c"), &all);
        let collection = files.collection(&mut Ids::default()).unwrap();
        (collection, files)
    }

    fn id(c: &Collection, path: &str) -> RequestId {
        c.entries()
            .iter()
            .find(|e| e.path() == Path::new(path))
            .unwrap()
            .id()
    }

    fn written(plan: &Plan, path: &str) -> String {
        plan.changes
            .iter()
            .find_map(|c| match c {
                Change::Write { path: p, bytes } if p == Path::new(path) => {
                    Some(String::from_utf8(bytes.clone()).unwrap())
                }
                _ => None,
            })
            .unwrap_or_else(|| panic!("no write to {path}: {plan:?}"))
    }

    fn rename(from: &str, to: &str) -> Change {
        Change::Rename {
            from: from.into(),
            to: to.into(),
        }
    }

    #[test]
    fn new_request_takes_a_free_slug() {
        let (c, files) = setup(&[("users/create-user.req.toml", &req("x"))]);

        let plan = plan(
            &c,
            &files,
            &Op::NewRequest {
                parent: "users".into(),
                name: "Create user".into(),
            },
        )
        .unwrap();

        assert_eq!(
            plan.select.as_deref(),
            Some(Path::new("users/create-user-2.req.toml")),
            "{plan:?}"
        );
        assert!(
            written(&plan, "users/create-user-2.req.toml").starts_with("name = \"Create user\"\n"),
            "{plan:?}"
        );
    }

    #[test]
    fn a_blank_name_becomes_request_or_folder() {
        let (c, files) = setup(&[]);

        let request = plan(
            &c,
            &files,
            &Op::NewRequest {
                parent: "".into(),
                name: "!!".into(),
            },
        )
        .unwrap();
        let folder = plan(
            &c,
            &files,
            &Op::NewFolder {
                parent: "".into(),
                name: "..".into(),
            },
        )
        .unwrap();

        assert_eq!(
            request.select.as_deref(),
            Some(Path::new("request.req.toml"))
        );
        assert_eq!(folder.select.as_deref(), Some(Path::new("folder")));
    }

    #[test]
    fn a_root_folder_named_envs_gets_another_name() {
        let (c, files) = setup(&[]);

        let plan = plan(
            &c,
            &files,
            &Op::NewFolder {
                parent: "".into(),
                name: "Envs".into(),
            },
        )
        .unwrap();

        assert_eq!(
            plan.changes,
            [Change::Write {
                path: "envs-2/_folder.toml".into(),
                bytes: b"name = \"Envs\"\n".to_vec()
            }]
        );
        assert_eq!(plan.select.as_deref(), Some(Path::new("envs-2")));
    }

    #[test]
    fn rename_moves_the_file_and_its_order_entry() {
        let (c, files) = setup(&[
            ("_folder.toml", "order = [\"b\", \"a\"]\n"),
            ("a.req.toml", &req("a")),
            ("b.req.toml", &req("b")),
        ]);

        let plan = plan(
            &c,
            &files,
            &Op::Rename {
                node: NodeRef::Request(id(&c, "a.req.toml")),
                name: "Zed".into(),
            },
        )
        .unwrap();

        assert_eq!(
            plan.changes.first(),
            Some(&rename("a.req.toml", "zed.req.toml"))
        );
        assert_eq!(written(&plan, "zed.req.toml"), req("Zed"));
        assert_eq!(written(&plan, "_folder.toml"), "order = [\"b\", \"zed\"]\n");
        assert_eq!(plan.select.as_deref(), Some(Path::new("zed.req.toml")));
    }

    #[test]
    fn rename_with_the_same_slug_only_sets_the_name() {
        let (c, files) = setup(&[("me.req.toml", &req("me"))]);

        let plan = plan(
            &c,
            &files,
            &Op::Rename {
                node: NodeRef::Request(id(&c, "me.req.toml")),
                name: "Me".into(),
            },
        )
        .unwrap();

        assert_eq!(
            plan.changes,
            [Change::Write {
                path: "me.req.toml".into(),
                bytes: req("Me").into_bytes()
            }]
        );
    }

    #[test]
    fn rename_onto_an_existing_file_is_exists() {
        let (c, files) = setup(&[("a.req.toml", &req("a")), ("b.req.toml", &req("b"))]);

        let err = plan(
            &c,
            &files,
            &Op::Rename {
                node: NodeRef::Request(id(&c, "a.req.toml")),
                name: "B".into(),
            },
        )
        .unwrap_err();

        assert!(err.text().starts_with("core::exists"), "{err:?}");
    }

    #[test]
    fn rename_never_leaves_the_folder() {
        let (c, files) = setup(&[("users/a.req.toml", &req("a"))]);

        let plan = plan(
            &c,
            &files,
            &Op::Rename {
                node: NodeRef::Request(id(&c, "users/a.req.toml")),
                name: "../../etc".into(),
            },
        )
        .unwrap();

        assert_eq!(
            plan.changes.first(),
            Some(&rename("users/a.req.toml", "users/etc.req.toml"))
        );
    }

    #[test]
    fn renaming_a_broken_request_is_invalid_file() {
        let (c, files) = setup(&[("a.req.toml", "name = ")]);

        let err = plan(
            &c,
            &files,
            &Op::Rename {
                node: NodeRef::Request(id(&c, "a.req.toml")),
                name: "A".into(),
            },
        )
        .unwrap_err();

        assert!(err.text().starts_with("model::invalid_file"), "{err:?}");
    }

    #[test]
    fn rename_folder_moves_the_directory_and_names_it() {
        let (c, files) = setup(&[
            ("_folder.toml", "order = [\"users\"]\n"),
            ("users/a.req.toml", &req("a")),
        ]);

        let plan = plan(
            &c,
            &files,
            &Op::Rename {
                node: NodeRef::Folder("users".into()),
                name: "People".into(),
            },
        )
        .unwrap();

        assert_eq!(plan.changes.first(), Some(&rename("users", "people")));
        assert_eq!(written(&plan, "people/_folder.toml"), "name = \"People\"\n");
        assert_eq!(written(&plan, "_folder.toml"), "order = [\"people\"]\n");
        assert_eq!(plan.select.as_deref(), Some(Path::new("people")));
    }

    #[test]
    fn a_name_without_letters_keeps_the_file_name() {
        let (c, files) = setup(&[("me.req.toml", &req("me"))]);

        let plan = plan(
            &c,
            &files,
            &Op::Rename {
                node: NodeRef::Request(id(&c, "me.req.toml")),
                name: "!!".into(),
            },
        )
        .unwrap();

        assert_eq!(
            plan.changes,
            [Change::Write {
                path: "me.req.toml".into(),
                bytes: req("!!").into_bytes()
            }]
        );
    }

    #[test]
    fn a_nested_folder_is_found_among_its_siblings() {
        let (c, files) = setup(&[("a/c/x.req.toml", &req("x")), ("a/b/y.req.toml", &req("y"))]);

        let plan = plan(
            &c,
            &files,
            &Op::Rename {
                node: NodeRef::Folder("a/c".into()),
                name: "D".into(),
            },
        )
        .unwrap();

        assert_eq!(plan.changes.first(), Some(&rename("a/c", "a/d")));
    }

    #[test]
    fn move_down_writes_the_full_order() {
        let (c, files) = setup(&[
            ("a.req.toml", &req("a")),
            ("b.req.toml", &req("b")),
            ("c.req.toml", &req("c")),
        ]);

        let plan = plan(
            &c,
            &files,
            &Op::Move {
                node: NodeRef::Request(id(&c, "a.req.toml")),
                dir: Direction::Down,
            },
        )
        .unwrap();

        assert_eq!(
            written(&plan, "_folder.toml"),
            "order = [\"b\", \"a\", \"c\"]\n"
        );
    }

    #[test]
    fn deleting_a_request_removes_its_sidecars() {
        let (c, files) = setup(&[
            ("users/create.req.toml", &req("c")),
            ("users/create.body.json", "{}"),
            ("users/created.req.toml", &req("d")),
            ("users/created.body.json", "{}"),
        ]);

        let plan = plan(
            &c,
            &files,
            &Op::Delete(NodeRef::Request(id(&c, "users/create.req.toml"))),
        )
        .unwrap();

        assert_eq!(
            plan.changes,
            [
                Change::Remove("users/create.req.toml".into()),
                Change::Remove("users/create.body.json".into())
            ]
        );
    }

    #[test]
    fn deleting_a_folder_keeps_other_files() {
        let (c, files) = setup(&[
            ("_folder.toml", "order = [\"a\", \"users\"]\n"),
            ("a.req.toml", &req("a")),
            ("users/_folder.toml", "name = \"U\"\n"),
            ("users/x.req.toml", &req("x")),
            ("users/sub/y.req.toml", &req("y")),
        ]);

        let plan = plan(&c, &files, &Op::Delete(NodeRef::Folder("users".into()))).unwrap();

        assert_eq!(
            plan.changes,
            [
                Change::Remove("users/_folder.toml".into()),
                Change::Remove("users/sub/y.req.toml".into()),
                Change::Remove("users/x.req.toml".into()),
                Change::RemoveDirIfEmpty("users/sub".into()),
                Change::RemoveDirIfEmpty("users".into()),
                Change::Write {
                    path: "_folder.toml".into(),
                    bytes: b"order = [\"a\"]\n".to_vec()
                },
            ],
            "only cached collection files are removed; notes.md on disk would stay"
        );
    }

    #[test]
    fn move_up_writes_the_full_order() {
        let (c, files) = setup(&[
            ("users/_folder.toml", "name = \"U\"\n"),
            ("users/a.req.toml", &req("a")),
            ("users/b.req.toml", &req("b")),
            ("users/c.req.toml", &req("c")),
        ]);

        let plan = plan(
            &c,
            &files,
            &Op::Move {
                node: NodeRef::Request(id(&c, "users/c.req.toml")),
                dir: Direction::Up,
            },
        )
        .unwrap();

        assert_eq!(
            plan.changes,
            [Change::Write {
                path: "users/_folder.toml".into(),
                bytes: b"name = \"U\"\norder = [\"a\", \"c\", \"b\"]\n".to_vec()
            }]
        );
    }

    #[test]
    fn move_at_the_edge_is_empty() {
        let (c, files) = setup(&[("a.req.toml", &req("a")), ("b.req.toml", &req("b"))]);

        let up = plan(
            &c,
            &files,
            &Op::Move {
                node: NodeRef::Request(id(&c, "a.req.toml")),
                dir: Direction::Up,
            },
        )
        .unwrap();
        let down = plan(
            &c,
            &files,
            &Op::Move {
                node: NodeRef::Request(id(&c, "b.req.toml")),
                dir: Direction::Down,
            },
        )
        .unwrap();

        assert_eq!((up, down), (Plan::default(), Plan::default()));
    }

    #[test]
    fn move_in_the_root_writes_the_root_folder_file() {
        let (c, files) = setup(&[("a.req.toml", &req("a")), ("users/x.req.toml", &req("x"))]);

        let plan = plan(
            &c,
            &files,
            &Op::Move {
                node: NodeRef::Folder("users".into()),
                dir: Direction::Up,
            },
        )
        .unwrap();

        assert_eq!(
            plan.changes,
            [Change::Write {
                path: "_folder.toml".into(),
                bytes: b"order = [\"users\", \"a\"]\n".to_vec()
            }]
        );
    }

    #[test]
    fn the_root_and_unknown_nodes_give_empty_plans() {
        let (c, files) = setup(&[("a.req.toml", &req("a"))]);

        for op in [
            Op::Delete(NodeRef::Folder(PathBuf::new())),
            Op::Delete(NodeRef::Request(RequestId::new(99))),
            Op::Rename {
                node: NodeRef::Folder("nope".into()),
                name: "x".into(),
            },
        ] {
            assert_eq!(plan(&c, &files, &op).unwrap(), Plan::default(), "{op:?}");
        }
    }
}
