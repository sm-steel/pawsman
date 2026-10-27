//! A collection: `pawsman.toml`, a tree of folders and requests, environments and `.env`.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use secrecy::SecretString;

use crate::folder::{Folder, FolderDefaults, Node};
use crate::{Environment, Manifest, ModelError, Request, RequestId, SourceFile, dotenv};

const MANIFEST: &str = "pawsman.toml";
const FOLDER_FILE: &str = "_folder.toml";
const REQUEST_EXT: &str = ".req.toml";
const ENV_EXT: &str = ".env.toml";
const ENVS: &str = "envs";

/// A collection and its entries.
#[derive(Debug, Clone)]
pub struct Collection {
    root: PathBuf,
    manifest: Manifest,
    tree: Folder,
    entries: Vec<Entry>,
    environments: Vec<Environment>,
    dotenv: Vec<(String, SecretString)>,
    issues: Vec<ModelError>,
}

/// One request file, parsed or not.
#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    id: RequestId,
    path: PathBuf,
    folder: PathBuf,
    kind: EntryKind,
}

/// What a file turned out to be.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum EntryKind {
    /// A valid request.
    Request(Box<Request>),
    /// A file that does not parse; shown as an error node.
    Invalid(ModelError),
}

impl Collection {
    /// Builds a collection from the files under `root` (paths relative to it).
    ///
    /// # Errors
    ///
    /// `model::not_a_collection` without a `pawsman.toml`; `model::invalid_file` or
    /// `model::format_too_new` for a bad one. Everything else broken becomes an error entry, a
    /// broken environment, or an issue.
    pub fn from_files(root: &Path, files: Vec<SourceFile>) -> Result<Self, ModelError> {
        let files: BTreeMap<PathBuf, Vec<u8>> =
            files.into_iter().map(SourceFile::into_parts).collect();
        let manifest_path = root.join(MANIFEST);
        let manifest = files
            .get(Path::new(MANIFEST))
            .ok_or_else(|| ModelError::not_a_collection(root))?;
        let manifest = Manifest::parse(utf8(manifest, &manifest_path)?, &manifest_path)?;
        let requests: Vec<&Path> = files
            .keys()
            .map(PathBuf::as_path)
            .filter(|p| is_request(p))
            .collect();
        let mut builder = Builder {
            files: &files,
            requests: &requests,
            format: manifest.format(),
            entries: Vec::new(),
        };
        let tree = builder.folder(Path::new(""), Some(manifest.name()), None);
        let entries = builder.entries;
        let mut issues = Vec::new();
        let environments = environments(&files, &mut issues);
        let mut values = Vec::new();
        if let Some(bytes) = files.get(Path::new(".env")) {
            match utf8(bytes, Path::new(".env")) {
                Ok(text) => {
                    let (pairs, problems) = dotenv::parse(text, Path::new(".env"));
                    values = pairs;
                    issues.extend(problems);
                }
                Err(err) => issues.push(err),
            }
        }
        Ok(Self {
            root: root.to_owned(),
            manifest,
            tree,
            entries,
            environments,
            dotenv: values,
            issues,
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

    /// The root folder; its children are the tree.
    #[must_use]
    pub fn root_folder(&self) -> &Folder {
        &self.tree
    }

    /// Every entry in tree order.
    #[must_use]
    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    /// The entry with `id`.
    #[must_use]
    pub fn entry(&self, id: RequestId) -> Option<&Entry> {
        self.entries.iter().find(|e| e.id == id)
    }

    /// The request with `id`, if it parsed.
    #[must_use]
    pub fn request(&self, id: RequestId) -> Option<&Request> {
        self.entry(id).and_then(|e| match &e.kind {
            EntryKind::Request(request) => Some(&**request),
            EntryKind::Invalid(_) => None,
        })
    }

    /// The folders from the root down to the one holding request `id`.
    #[must_use]
    pub fn chain(&self, id: RequestId) -> Vec<&Folder> {
        let mut chain = vec![&self.tree];
        let Some(entry) = self.entry(id) else {
            return chain;
        };
        let mut current = &self.tree;
        let mut prefix = PathBuf::new();
        for part in entry.folder.components() {
            prefix.push(part);
            let next = current.children().iter().find_map(|n| match n {
                Node::Folder(f) if f.path() == prefix => Some(&**f),
                _ => None,
            });
            let Some(next) = next else {
                break;
            };
            chain.push(next);
            current = next;
        }
        chain
    }

    /// Environments sorted by name, broken ones included.
    #[must_use]
    pub fn environments(&self) -> &[Environment] {
        &self.environments
    }

    /// The values in `.env`.
    #[must_use]
    pub fn dotenv(&self) -> &[(String, SecretString)] {
        &self.dotenv
    }

    /// Problems that belong to no entry: bad `.env` lines, ignored environment files.
    #[must_use]
    pub fn issues(&self) -> &[ModelError] {
        &self.issues
    }
}

/// Walks the folder tree, numbering requests in display order.
struct Builder<'a> {
    files: &'a BTreeMap<PathBuf, Vec<u8>>,
    requests: &'a [&'a Path],
    format: u32,
    entries: Vec<Entry>,
}

/// A folder's child before ordering.
enum Child {
    Folder(PathBuf),
    Request(PathBuf),
}

impl Builder<'_> {
    fn folder(&mut self, dir: &Path, name: Option<&str>, inherited: Option<&ModelError>) -> Folder {
        let file = dir.join(FOLDER_FILE);
        let (defaults, error) = match self.files.get(&file) {
            None => (FolderDefaults::default(), None),
            Some(bytes) => match utf8(bytes, &file)
                .and_then(|text| FolderDefaults::parse(text, &file, self.format))
            {
                Ok(defaults) => (defaults, None),
                Err(err) => (FolderDefaults::default(), Some(err)),
            },
        };
        let broken = inherited.cloned().or_else(|| error.clone());
        let mut children = Vec::new();
        for child in self.children(dir, &defaults.order) {
            match child {
                Child::Folder(path) => {
                    let folder = self.folder(&path, None, broken.as_ref());
                    children.push(Node::Folder(Box::new(folder)));
                }
                Child::Request(path) => {
                    let id = self.request(&path, dir, broken.as_ref());
                    children.push(Node::Entry(id));
                }
            }
        }
        let name = name
            .map(str::to_owned)
            .or(defaults.name)
            .unwrap_or_else(|| file_name(dir));
        Folder {
            name,
            path: dir.to_owned(),
            headers: defaults.headers,
            vars: defaults.vars,
            auth: defaults.auth,
            children,
            error,
        }
    }

    /// The requests and subfolders directly in `dir`: `order` first, then by name.
    fn children(&self, dir: &Path, order: &[String]) -> Vec<Child> {
        let mut folders = BTreeSet::new();
        let mut found: Vec<(String, Child)> = Vec::new();
        for path in self.requests {
            let Ok(rest) = path.strip_prefix(dir) else {
                continue;
            };
            let mut parts = rest.components();
            let (Some(first), more) = (parts.next(), parts.next().is_some()) else {
                continue;
            };
            let first = first.as_os_str().to_string_lossy().into_owned();
            if more {
                if folders.insert(first.clone()) {
                    found.push((first.clone(), Child::Folder(dir.join(&first))));
                }
            } else {
                let stem = first.strip_suffix(REQUEST_EXT).unwrap_or(&first).to_owned();
                found.push((stem, Child::Request((*path).to_owned())));
            }
        }
        let rank = |key: &str| order.iter().position(|o| o == key).unwrap_or(usize::MAX);
        found.sort_by(|(a, _), (b, _)| rank(a).cmp(&rank(b)).then_with(|| a.cmp(b)));
        found.into_iter().map(|(_, child)| child).collect()
    }

    fn request(&mut self, path: &Path, dir: &Path, broken: Option<&ModelError>) -> RequestId {
        let id = RequestId::new(u32::try_from(self.entries.len() + 1).unwrap_or(u32::MAX));
        let kind = if let Some(err) = broken {
            EntryKind::Invalid(err.clone())
        } else {
            let bytes = self.files.get(path).map_or(&[][..], Vec::as_slice);
            let sidecar = |name: &str| self.files.get(&dir.join(name)).cloned();
            match utf8(bytes, path).and_then(|text| Request::parse(text, path, &sidecar)) {
                Ok(request) => EntryKind::Request(Box::new(request)),
                Err(err) => EntryKind::Invalid(err),
            }
        };
        self.entries.push(Entry {
            id,
            path: path.to_owned(),
            folder: dir.to_owned(),
            kind,
        });
        id
    }
}

/// `*.req.toml` anywhere but under `envs/`.
fn is_request(path: &Path) -> bool {
    let named = path
        .file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|n| n.ends_with(REQUEST_EXT));
    named && !path.starts_with(ENVS)
}

/// `envs/<name>.env.toml`, sorted by name; badly named files become issues.
fn environments(
    files: &BTreeMap<PathBuf, Vec<u8>>,
    issues: &mut Vec<ModelError>,
) -> Vec<Environment> {
    let mut out = Vec::new();
    for (path, bytes) in files {
        if path.parent() != Some(Path::new(ENVS)) {
            continue;
        }
        let Some(name) = path
            .file_name()
            .and_then(|n| n.to_str())
            .and_then(|n| n.strip_suffix(ENV_EXT))
        else {
            continue;
        };
        let valid = !name.is_empty()
            && name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-'));
        if !valid {
            issues.push(ModelError::invalid_file(
                path,
                "an environment name may hold only letters, digits, `_` and `-`",
            ));
            continue;
        }
        out.push(Environment::parse(name, utf8(bytes, path), path));
    }
    out.sort_by(|a, b| a.name().cmp(b.name()));
    out
}

fn utf8<'a>(bytes: &'a [u8], path: &Path) -> Result<&'a str, ModelError> {
    std::str::from_utf8(bytes).map_err(|_| ModelError::invalid_file(path, "not UTF-8"))
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map_or_else(String::new, |n| n.to_string_lossy().into_owned())
}

impl Entry {
    /// The entry's id.
    #[must_use]
    pub fn id(&self) -> RequestId {
        self.id
    }

    /// The file, relative to the collection root.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The folder holding the file, relative to the root.
    #[must_use]
    pub fn folder(&self) -> &Path {
        &self.folder
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
    use crate::folder::Node;

    const MANIFEST: &str = "format = 1\nname = \"Demo\"\n";

    fn req(name: &str) -> String {
        format!("name = \"{name}\"\nurl = \"http://h/{name}\"\n")
    }

    fn build(files: &[(&str, &[u8])]) -> Result<Collection, ModelError> {
        Collection::from_files(
            Path::new("/c"),
            files
                .iter()
                .map(|(p, b)| SourceFile::new(PathBuf::from(p), b.to_vec()))
                .collect(),
        )
    }

    fn walk(folder: &Folder, c: &Collection, out: &mut Vec<String>) {
        for node in folder.children() {
            match node {
                Node::Folder(f) => {
                    out.push(format!("{}/", f.path().display()));
                    walk(f, c, out);
                }
                Node::Entry(id) => {
                    out.push(format!("{} {}", id, c.entry(*id).unwrap().path().display()));
                }
            }
        }
    }

    fn tree(c: &Collection) -> Vec<String> {
        let mut out = Vec::new();
        walk(c.root_folder(), c, &mut out);
        out
    }

    #[test]
    fn no_manifest_is_not_a_collection() {
        let err = build(&[("a.req.toml", req("a").as_bytes())]).unwrap_err();

        assert_eq!(err.code(), crate::codes::NOT_A_COLLECTION, "code");
        assert!(err.to_string().contains("/c"), "names the directory: {err}");
    }

    #[test]
    fn folders_nest_and_order_puts_named_children_first() {
        let c = build(&[
            ("pawsman.toml", MANIFEST.as_bytes()),
            ("_folder.toml", b"order = [\"b\", \"sub\", \"gone\"]\n"),
            ("a.req.toml", req("a").as_bytes()),
            ("b.req.toml", req("b").as_bytes()),
            ("c.req.toml", req("c").as_bytes()),
            ("sub/x.req.toml", req("x").as_bytes()),
        ])
        .unwrap();

        assert_eq!(
            tree(&c),
            [
                "#1 b.req.toml",
                "sub/",
                "#2 sub/x.req.toml",
                "#3 a.req.toml",
                "#4 c.req.toml"
            ],
        );
        let ids: Vec<_> = c.entries().iter().map(super::Entry::id).collect();
        assert_eq!(
            ids,
            (1..=4).map(RequestId::new).collect::<Vec<_>>(),
            "flat list in walk order"
        );
        assert_eq!(c.name(), "Demo", "collection name");
    }

    #[test]
    fn unordered_children_sort_by_name_folders_and_requests_mixed() {
        let c = build(&[
            ("pawsman.toml", MANIFEST.as_bytes()),
            ("b.req.toml", req("b").as_bytes()),
            ("a/x.req.toml", req("x").as_bytes()),
            ("c/y.req.toml", req("y").as_bytes()),
        ])
        .unwrap();

        assert_eq!(
            tree(&c),
            [
                "a/",
                "#1 a/x.req.toml",
                "#2 b.req.toml",
                "c/",
                "#3 c/y.req.toml"
            ]
        );
    }

    #[test]
    fn chain_lists_folders_root_first() {
        let c = build(&[
            ("pawsman.toml", MANIFEST.as_bytes()),
            (
                "users/_folder.toml",
                b"name = \"Users\"\n[headers]\nA = \"1\"\n",
            ),
            ("users/admin/x.req.toml", req("x").as_bytes()),
        ])
        .unwrap();

        let chain: Vec<_> = c
            .chain(RequestId::new(1))
            .iter()
            .map(|f| (f.name().to_owned(), f.path().to_owned()))
            .collect();
        assert_eq!(
            chain,
            [
                ("Demo".to_owned(), PathBuf::new()),
                ("Users".to_owned(), PathBuf::from("users")),
                ("admin".to_owned(), PathBuf::from("users/admin"))
            ]
        );
        assert_eq!(
            c.chain(RequestId::new(1)).get(1).map(|f| f.headers().len()),
            Some(1),
            "folder headers"
        );
        assert_eq!(
            c.entry(RequestId::new(1)).map(Entry::folder),
            Some(Path::new("users/admin")),
            "entry folder"
        );
    }

    #[test]
    fn a_broken_folder_file_invalidates_its_requests() {
        let c = build(&[
            ("pawsman.toml", MANIFEST.as_bytes()),
            ("users/_folder.toml", b"title = \"unknown key\"\n"),
            ("users/a.req.toml", req("a").as_bytes()),
            ("users/deep/b.req.toml", req("b").as_bytes()),
            ("ok.req.toml", req("ok").as_bytes()),
        ])
        .unwrap();

        let invalid: Vec<_> = c
            .entries()
            .iter()
            .map(|e| (e.path().display().to_string(), matches!(e.kind(), EntryKind::Invalid(err) if err.to_string().contains("_folder.toml"))))
            .collect();
        assert_eq!(
            invalid,
            [
                ("ok.req.toml".to_owned(), false),
                ("users/a.req.toml".to_owned(), true),
                ("users/deep/b.req.toml".to_owned(), true)
            ]
        );
        let users = c.chain(RequestId::new(2)).get(1).copied().unwrap();
        assert!(users.error().is_some(), "the folder carries its error");
    }

    #[test]
    fn a_missing_sidecar_is_an_error_entry() {
        let c = build(&[
            ("pawsman.toml", MANIFEST.as_bytes()),
            (
                "a.req.toml",
                b"name = \"a\"\nurl = \"u\"\n[body]\njson = { sidecar = \"a.body.json\" }\n",
            ),
        ])
        .unwrap();

        assert!(
            matches!(c.entries().first().map(Entry::kind), Some(EntryKind::Invalid(e)) if e.code() == crate::codes::MISSING_SIDECAR),
            "{:?}",
            c.entries()
        );
    }

    #[test]
    fn a_sidecar_in_a_subfolder_is_found_next_to_its_request() {
        let c = build(&[
            ("pawsman.toml", MANIFEST.as_bytes()),
            (
                "users/create.req.toml",
                b"name = \"c\"\nurl = \"u\"\n[body]\njson = { sidecar = \"create.body.json\" }\n",
            ),
            ("users/create.body.json", b"{\"a\": 1}"),
        ])
        .unwrap();

        assert_eq!(
            c.request(RequestId::new(1)).and_then(Request::body_text),
            Some("{\"a\": 1}")
        );
    }

    #[test]
    fn files_that_are_not_utf8_are_invalid() {
        let c = build(&[
            ("pawsman.toml", MANIFEST.as_bytes()),
            ("bad.req.toml", &[0xff, 0xfe]),
        ])
        .unwrap();
        let manifest = build(&[("pawsman.toml", &[0xff])]).unwrap_err();

        assert!(
            matches!(c.entries().first().map(Entry::kind), Some(EntryKind::Invalid(e)) if e.to_string().contains("not UTF-8")),
            "{:?}",
            c.entries()
        );
        assert_eq!(manifest.code(), crate::codes::INVALID_FILE, "manifest");
    }

    #[test]
    fn environments_sorted_and_broken_ones_kept_with_their_error() {
        let c = build(&[
            ("pawsman.toml", MANIFEST.as_bytes()),
            (
                "envs/prod.env.toml",
                b"secrets = [\"token\"]\n[vars]\nbase = \"https://p\"\n",
            ),
            ("envs/dev.env.toml", b"[vars]\nbase = \"http://d\"\n"),
            ("envs/broken.env.toml", b"vars = 1\n"),
            ("envs/bad name.env.toml", b""),
        ])
        .unwrap();

        let envs: Vec<_> = c
            .environments()
            .iter()
            .map(|e| (e.name(), e.error().is_some()))
            .collect();
        assert_eq!(envs, [("broken", true), ("dev", false), ("prod", false)]);
        let prod = c.environments().get(2).unwrap();
        assert_eq!(prod.secrets(), ["token"], "secrets");
        assert_eq!(prod.vars(), &[("base".into(), "https://p".into())], "vars");
        assert!(
            c.issues()
                .iter()
                .any(|i| i.to_string().contains("bad name")),
            "bad name reported: {:?}",
            c.issues()
        );
    }

    #[test]
    fn dotenv_values_and_issues_are_collected() {
        let c = build(&[
            ("pawsman.toml", MANIFEST.as_bytes()),
            (".env", b"token=abc\nnot a line\n"),
        ])
        .unwrap();

        assert_eq!(c.dotenv().len(), 1, "values");
        assert_eq!(c.issues().len(), 1, "issues");
    }

    #[test]
    fn unrelated_files_are_ignored() {
        let c = build(&[
            ("pawsman.toml", MANIFEST.as_bytes()),
            ("README.md", b"# hi"),
            ("notes/todo.txt", b"x"),
            ("envs/x.req.toml", req("x").as_bytes()),
            ("a.req.toml", req("a").as_bytes()),
        ])
        .unwrap();

        assert_eq!(
            tree(&c),
            ["#1 a.req.toml"],
            "no empty folders, no requests in envs/"
        );
    }

    #[test]
    fn a_broken_manifest_is_an_invalid_file() {
        for manifest in ["name = \"x\"\n", "format = 1\nname = \"x\"\nextra = 1\n"] {
            let err = build(&[("pawsman.toml", manifest.as_bytes())]).unwrap_err();
            assert_eq!(err.code(), crate::codes::INVALID_FILE, "{manifest:?}");
        }
        let newer = build(&[("pawsman.toml", b"format = 2\nname = \"x\"\n")]).unwrap_err();
        assert_eq!(newer.code(), crate::codes::FORMAT_TOO_NEW, "newer format");
    }
}
