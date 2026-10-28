//! The sidebar tree as rows: which nodes are visible given the collapsed folders, and how the
//! cursor moves over them.

use std::collections::BTreeSet;
use std::path::PathBuf;

use pawsman_core::tree::NodeRef;
use pawsman_model::Collection;

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
    let _ = (collection, collapsed);
    Vec::new()
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
                folder("a/sub", 1),
                request(1, 2),
                request(2, 1),
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
                folder("a/sub", 1),
                request(2, 1),
                request(3, 0)
            ]
        );
    }
}
