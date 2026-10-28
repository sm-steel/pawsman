//! Tree operations: create, rename, delete and reorder requests and folders. `plan` is pure: it
//! turns an operation into file changes from the cached bytes; `store::apply` runs them.

use std::path::{Path, PathBuf};

use pawsman_model::{Collection, Folder, Node, RequestId};

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
    let _ = (collection, files, op);
    Ok(Plan::default())
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
