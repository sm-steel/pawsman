//! Folders: directories of requests, with optional `_folder.toml` defaults and order.

use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::manifest::strings;
use crate::{ModelError, RequestId};

/// A folder of the collection tree; the root folder is the collection itself.
#[derive(Debug, Clone, PartialEq)]
pub struct Folder {
    pub(crate) name: String,
    pub(crate) path: PathBuf,
    pub(crate) headers: Vec<(String, String)>,
    pub(crate) vars: Vec<(String, String)>,
    pub(crate) auth: Option<toml::Table>,
    pub(crate) children: Vec<Node>,
    pub(crate) error: Option<ModelError>,
}

/// A child of a folder.
#[derive(Debug, Clone, PartialEq)]
pub enum Node {
    /// A subfolder.
    Folder(Box<Folder>),
    /// A request file; look it up with [`crate::Collection::entry`].
    Entry(RequestId),
}

impl Folder {
    /// The display name: `_folder.toml`'s `name`, else the directory name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The path relative to the collection root; empty for the root.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Default headers for requests below.
    #[must_use]
    pub fn headers(&self) -> &[(String, String)] {
        &self.headers
    }

    /// Variables for requests below.
    #[must_use]
    pub fn vars(&self) -> &[(String, String)] {
        &self.vars
    }

    /// Default `[auth]`, applied from M3.
    #[must_use]
    pub fn auth(&self) -> Option<&toml::Table> {
        self.auth.as_ref()
    }

    /// Children in display order.
    #[must_use]
    pub fn children(&self) -> &[Node] {
        &self.children
    }

    /// Why `_folder.toml` is not usable; its requests are error entries then.
    #[must_use]
    pub fn error(&self) -> Option<&ModelError> {
        self.error.as_ref()
    }
}

/// What `_folder.toml` holds.
#[derive(Debug, Default)]
pub(crate) struct FolderDefaults {
    pub(crate) name: Option<String>,
    pub(crate) order: Vec<String>,
    pub(crate) headers: Vec<(String, String)>,
    pub(crate) vars: Vec<(String, String)>,
    pub(crate) auth: Option<toml::Table>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FolderFile {
    name: Option<String>,
    order: Option<Vec<String>>,
    headers: Option<toml::Table>,
    vars: Option<toml::Table>,
    auth: Option<toml::Table>,
}

impl FolderDefaults {
    /// Parses `_folder.toml`.
    pub(crate) fn parse(text: &str, path: &Path, format: u32) -> Result<Self, ModelError> {
        let table: toml::Table =
            toml::from_str(text).map_err(|e| ModelError::invalid_file(path, e.message()))?;
        let table = crate::format::migrate(table, format, path)?;
        let file = FolderFile::deserialize(table)
            .map_err(|e| ModelError::invalid_file(path, e.message()))?;
        Ok(Self {
            name: file.name,
            order: file.order.unwrap_or_default(),
            headers: strings(file.headers, "headers", path)?,
            vars: strings(file.vars, "vars", path)?,
            auth: file.auth,
        })
    }
}
