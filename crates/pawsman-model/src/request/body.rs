//! A request body: inline, in a sidecar file, a form, multipart parts or a file.

use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::ModelError;
use crate::manifest::strings;

/// What `[body]` holds.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum Body {
    /// JSON text, kept as written.
    Json(Content),
    /// Plain text.
    Text(Content),
    /// `application/x-www-form-urlencoded` fields, in file order.
    Form(Vec<(String, String)>),
    /// `multipart/form-data` parts.
    Multipart(Vec<Part>),
    /// A file sent as is, relative to the request file.
    File(PathBuf),
}

/// The bytes of a JSON or text body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Content {
    /// UTF-8 text; templates apply.
    Utf8(String),
    /// Bytes that are not UTF-8, from a sidecar; sent as is.
    Binary(Vec<u8>),
}

/// One multipart part.
#[derive(Debug, Clone, PartialEq)]
pub struct Part {
    pub(crate) name: String,
    pub(crate) data: PartData,
    pub(crate) content_type: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PartData {
    Text(String),
    File(PathBuf),
}

/// What a part carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PartValue<'a> {
    /// A text value; templates apply.
    Text(&'a str),
    /// A file, relative to the request file.
    File(&'a Path),
}

impl Part {
    /// The field name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The value or file.
    #[must_use]
    pub fn value(&self) -> PartValue<'_> {
        match &self.data {
            PartData::Text(text) => PartValue::Text(text),
            PartData::File(path) => PartValue::File(path),
        }
    }

    /// The part's own `Content-Type`.
    #[must_use]
    pub fn content_type(&self) -> Option<&str> {
        self.content_type.as_deref()
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct BodyFile {
    json: Option<toml::Value>,
    text: Option<toml::Value>,
    form: Option<toml::Table>,
    multipart: Option<Vec<PartFile>>,
    file: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PartFile {
    name: String,
    value: Option<String>,
    file: Option<String>,
    content_type: Option<String>,
}

/// Reads the bytes of a sidecar file next to the request.
pub(crate) type Sidecars<'a> = &'a dyn Fn(&str) -> Option<Vec<u8>>;

impl BodyFile {
    pub(crate) fn into_body(self, path: &Path, sidecar: Sidecars<'_>) -> Result<Body, ModelError> {
        let given = [
            self.json.is_some(),
            self.text.is_some(),
            self.form.is_some(),
            self.multipart.is_some(),
            self.file.is_some(),
        ];
        if given.iter().filter(|g| **g).count() != 1 {
            return Err(ModelError::invalid_file(
                path,
                "`[body]` holds exactly one of json, text, form, multipart, file",
            ));
        }
        if let Some(json) = self.json {
            return Ok(Body::Json(content(json, "body.json", path, sidecar)?));
        }
        if let Some(text) = self.text {
            return Ok(Body::Text(content(text, "body.text", path, sidecar)?));
        }
        if let Some(form) = self.form {
            return Ok(Body::Form(strings(Some(form), "body.form", path)?));
        }
        if let Some(parts) = self.multipart {
            return parts
                .into_iter()
                .map(|p| p.into_part(path))
                .collect::<Result<_, _>>()
                .map(Body::Multipart);
        }
        Ok(Body::File(PathBuf::from(self.file.unwrap_or_default())))
    }
}

impl PartFile {
    fn into_part(self, path: &Path) -> Result<Part, ModelError> {
        let data = match (self.value, self.file) {
            (Some(value), None) => PartData::Text(value),
            (None, Some(file)) => PartData::File(PathBuf::from(file)),
            _ => {
                return Err(ModelError::invalid_file(
                    path,
                    &format!(
                        "multipart part `{}` needs exactly one of value, file",
                        self.name
                    ),
                ));
            }
        };
        Ok(Part {
            name: self.name,
            data,
            content_type: self.content_type,
        })
    }
}

/// An inline string or `{ sidecar = "name" }`.
fn content(
    value: toml::Value,
    key: &str,
    path: &Path,
    sidecar: Sidecars<'_>,
) -> Result<Content, ModelError> {
    let name = match value {
        toml::Value::String(text) => return Ok(Content::Utf8(text)),
        toml::Value::Table(mut table) if table.len() == 1 => match table.remove("sidecar") {
            Some(toml::Value::String(name)) => name,
            _ => return Err(bad_content(key, path)),
        },
        _ => return Err(bad_content(key, path)),
    };
    let plain = !name.is_empty() && !name.contains(['/', '\\']) && name != ".." && name != ".";
    if !plain {
        return Err(ModelError::invalid_file(
            path,
            &format!("`{key}.sidecar` must be a file name next to the request"),
        ));
    }
    let bytes = sidecar(&name).ok_or_else(|| ModelError::missing_sidecar(path, &name))?;
    Ok(match String::from_utf8(bytes) {
        Ok(text) => Content::Utf8(text),
        Err(err) => Content::Binary(err.into_bytes()),
    })
}

fn bad_content(key: &str, path: &Path) -> ModelError {
    ModelError::invalid_file(
        path,
        &format!("`{key}` must be a string or `{{ sidecar = \"file\" }}`"),
    )
}
