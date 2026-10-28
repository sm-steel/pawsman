//! The sidebar tree as rows: which nodes are visible given the collapsed folders, and how the
//! cursor moves over them.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use pawsman_core::tree::NodeRef;
use pawsman_model::{Collection, Folder, Node};

/// One visible row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Row {
    /// The node.
    pub(crate) node: NodeRef,
    /// How deep it sits; the root's children are 0.
    pub(crate) depth: usize,
}

/// The visible rows in display order; a collapsed folder shows, its children do not.
pub(crate) fn rows(collection: &Collection, collapsed: &BTreeSet<PathBuf>) -> Vec<Row> {
    let mut rows = Vec::new();
    push(collection.root_folder(), 0, collapsed, &mut rows);
    rows
}

fn push(folder: &Folder, depth: usize, collapsed: &BTreeSet<PathBuf>, rows: &mut Vec<Row>) {
    for node in folder.children() {
        match node {
            Node::Folder(child) => {
                rows.push(Row {
                    node: NodeRef::Folder(child.path().to_owned()),
                    depth,
                });
                if !collapsed.contains(child.path()) {
                    push(child, depth + 1, collapsed, rows);
                }
            }
            Node::Entry(id) => rows.push(Row {
                node: NodeRef::Request(*id),
                depth,
            }),
        }
    }
}

/// The folder at `path`; the root is the empty path.
pub(crate) fn folder<'a>(collection: &'a Collection, path: &Path) -> Option<&'a Folder> {
    let mut current = collection.root_folder();
    for part in path.components() {
        let prefix = current.path().join(part);
        current = current.children().iter().find_map(|n| match n {
            Node::Folder(f) if f.path() == prefix => Some(&**f),
            _ => None,
        })?;
    }
    Some(current)
}

/// The node at a path a tree operation reported: a request file or a folder directory.
pub(crate) fn node_at(collection: &Collection, path: &Path) -> Option<NodeRef> {
    if let Some(entry) = collection.entries().iter().find(|e| e.path() == path) {
        return Some(NodeRef::Request(entry.id()));
    }
    folder(collection, path).map(|f| NodeRef::Folder(f.path().to_owned()))
}

/// The folder holding `node`; the root holds the top level.
pub(crate) fn parent(collection: &Collection, node: &NodeRef) -> PathBuf {
    match node {
        NodeRef::Request(id) => collection
            .entry(*id)
            .map(|e| e.folder().to_owned())
            .unwrap_or_default(),
        NodeRef::Folder(path) => path.parent().map(Path::to_owned).unwrap_or_default(),
    }
}

/// The requests in a folder and every folder below it.
pub(crate) fn count(folder: &Folder) -> usize {
    folder
        .children()
        .iter()
        .map(|n| match n {
            Node::Folder(f) => count(f),
            Node::Entry(_) => 1,
        })
        .sum()
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use pawsman_model::{RequestId, SourceFile};

    use super::*;

    fn collection() -> Collection {
        let files = [
            ("pawsman.toml", "format = 1\nname = \"C\"\n"),
            ("a/one.req.toml", "name = \"One\"\nurl = \"u\"\n"),
            ("a/sub/deep.req.toml", "name = \"Deep\"\nurl = \"u\"\n"),
            ("top.req.toml", "name = \"Top\"\nurl = \"u\"\n"),
        ];
        Collection::from_files(
            Path::new("/c"),
            files
                .iter()
                .map(|(p, t)| SourceFile::new(PathBuf::from(p), t.as_bytes().to_vec()))
                .collect(),
        )
        .unwrap()
    }

    fn folder(path: &str, depth: usize) -> Row {
        Row {
            node: NodeRef::Folder(path.into()),
            depth,
        }
    }

    fn request(id: u32, depth: usize) -> Row {
        Row {
            node: NodeRef::Request(RequestId::new(id)),
            depth,
        }
    }

    #[test]
    fn rows_list_folders_and_requests_in_order() {
        let rows = rows(&collection(), &BTreeSet::new());

        assert_eq!(
            rows,
            [
                folder("a", 0),
                request(1, 1),
                folder("a/sub", 1),
                request(2, 2),
                request(3, 0)
            ]
        );
    }

    #[test]
    fn rows_hide_children_of_collapsed_folders() {
        let collapsed = BTreeSet::from([PathBuf::from("a/sub")]);

        let rows = rows(&collection(), &collapsed);

        assert_eq!(
            rows,
            [
                folder("a", 0),
                request(1, 1),
                folder("a/sub", 1),
                request(3, 0)
            ]
        );
    }
}
