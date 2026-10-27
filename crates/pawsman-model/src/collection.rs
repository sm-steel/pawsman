//! A collection: `pawsman.toml` plus its request files.

use std::path::{Path, PathBuf};

use crate::{Manifest, ModelError, Request, RequestId};

const MANIFEST: &str = "pawsman.toml";

/// A collection and its entries.
#[derive(Debug, Clone, PartialEq)]
pub struct Collection {
    root: PathBuf,
    manifest: Manifest,
    entries: Vec<Entry>,
}

/// One request file, parsed or not.
#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    id: RequestId,
    path: PathBuf,
    kind: EntryKind,
}

/// What a file turned out to be.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum EntryKind {
    /// A valid request.
    Request(Request),
    /// A file that does not parse; shown as an error node.
    Invalid(ModelError),
}

impl Collection {
    /// Builds a collection from the manifest text and the request files' contents.
    ///
    /// # Errors
    ///
    /// `model::not_a_collection` without a manifest; `model::invalid_file` for a bad manifest.
    pub fn from_files(
        root: &Path,
        manifest: Option<&str>,
        files: Vec<(PathBuf, String)>,
    ) -> Result<Self, ModelError> {
        let manifest = manifest.ok_or_else(|| ModelError::not_a_collection(root))?;
        let manifest_path = root.join(MANIFEST);
        let manifest = Manifest::parse(manifest, &manifest_path)?;
        let mut files = files;
        files.sort_by(|a, b| a.0.file_name().cmp(&b.0.file_name()));
        let entries = (1..)
            .zip(files)
            .map(|(n, (path, text))| {
                let kind = match Request::parse(&text, &path, &|_| None) {
                    Ok(request) => EntryKind::Request(request),
                    Err(err) => EntryKind::Invalid(err),
                };
                Entry {
                    id: RequestId::new(n),
                    path,
                    kind,
                }
            })
            .collect();
        Ok(Self {
            root: root.to_owned(),
            manifest,
            entries,
        })
    }

    /// The collection directory.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The display name.
    #[must_use]
    pub fn name(&self) -> &str {
        self.manifest.name()
    }

    /// The parsed `pawsman.toml`.
    #[must_use]
    pub fn manifest(&self) -> &Manifest {
        &self.manifest
    }

    /// Every entry, sorted by file name.
    #[must_use]
    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    /// The request with `id`, if it parsed.
    #[must_use]
    pub fn request(&self, id: RequestId) -> Option<&Request> {
        self.entries
            .iter()
            .find(|e| e.id == id)
            .and_then(|e| match &e.kind {
                EntryKind::Request(request) => Some(request),
                EntryKind::Invalid(_) => None,
            })
    }
}

impl Entry {
    /// The entry's id.
    #[must_use]
    pub fn id(&self) -> RequestId {
        self.id
    }

    /// The file.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The parsed request or its error.
    #[must_use]
    pub fn kind(&self) -> &EntryKind {
        &self.kind
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use pawsman_error::ErrorInfo as _;

    use super::*;

    fn file(name: &str, text: &str) -> (PathBuf, String) {
        (PathBuf::from(format!("/c/{name}")), text.to_owned())
    }

    const MANIFEST: &str = "format = 1\nname = \"Demo\"\n";

    #[test]
    fn no_manifest_is_not_a_collection() {
        let err = Collection::from_files(Path::new("/c"), None, vec![]).unwrap_err();

        assert_eq!(err.code(), crate::codes::NOT_A_COLLECTION, "code");
        assert!(err.to_string().contains("/c"), "names the directory: {err}");
    }

    #[test]
    fn entries_are_sorted_by_file_name_with_ids_from_one() {
        let c = Collection::from_files(
            Path::new("/c"),
            Some(MANIFEST),
            vec![
                file("b.req.toml", "name = \"B\"\nurl = \"http://h/b\"\n"),
                file("a.req.toml", "name = \"A\"\nurl = \"http://h/a\"\n"),
            ],
        )
        .unwrap();

        let names: Vec<_> = c
            .entries()
            .iter()
            .map(|e| e.path().file_name().unwrap().to_owned())
            .collect();
        assert_eq!(c.name(), "Demo", "collection name");
        assert_eq!(names, ["a.req.toml", "b.req.toml"], "sorted");
        assert_eq!(
            c.request(RequestId::new(1)).map(Request::name),
            Some("A"),
            "ids from 1"
        );
        assert_eq!(c.entries()[1].id(), RequestId::new(2), "second id");
    }

    #[test]
    fn a_broken_file_becomes_an_error_entry() {
        let c = Collection::from_files(
            Path::new("/c"),
            Some(MANIFEST),
            vec![
                file("good.req.toml", "name = \"G\"\nurl = \"http://h\"\n"),
                file("bad.req.toml", "name = "),
            ],
        )
        .unwrap();

        let invalid: Vec<bool> = c
            .entries()
            .iter()
            .map(|e| matches!(e.kind(), EntryKind::Invalid(_)))
            .collect();
        assert_eq!(
            invalid,
            [true, false],
            "bad first (sorted), good still usable"
        );
        assert!(
            c.request(RequestId::new(1)).is_none(),
            "the broken one has no request"
        );
        assert!(c.request(RequestId::new(2)).is_some(), "the good one sends");
    }

    #[test]
    fn a_broken_manifest_is_an_invalid_file() {
        for manifest in ["name = \"x\"\n", "format = 1\nname = \"x\"\nextra = 1\n"] {
            let err = Collection::from_files(Path::new("/c"), Some(manifest), vec![]).unwrap_err();
            assert_eq!(err.code(), crate::codes::INVALID_FILE, "{manifest:?}");
        }
        let newer =
            Collection::from_files(Path::new("/c"), Some("format = 2\nname = \"x\"\n"), vec![])
                .unwrap_err();
        assert_eq!(newer.code(), crate::codes::FORMAT_TOO_NEW, "newer format");
    }
}
