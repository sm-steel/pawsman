//! `envs/<name>.env.toml`: an environment's variables and the names of its secrets.

use std::path::Path;

use serde::Deserialize;

use crate::ModelError;
use crate::manifest::strings;

/// One environment.
#[derive(Debug, Clone, PartialEq)]
pub struct Environment {
    name: String,
    vars: Vec<(String, String)>,
    secrets: Vec<String>,
    error: Option<ModelError>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EnvironmentFile {
    vars: Option<toml::Table>,
    secrets: Option<Vec<String>>,
}

impl Environment {
    /// Parses the file of environment `name`; a broken file gives an environment with its error.
    #[must_use]
    pub(crate) fn parse(name: &str, text: Result<&str, ModelError>, path: &Path) -> Self {
        let broken = |error| Self {
            name: name.to_owned(),
            vars: Vec::new(),
            secrets: Vec::new(),
            error: Some(error),
        };
        let text = match text {
            Ok(text) => text,
            Err(err) => return broken(err),
        };
        let parsed = toml::from_str::<EnvironmentFile>(text)
            .map_err(|e| ModelError::invalid_file(path, e.message()))
            .and_then(|file| {
                Ok((
                    strings(file.vars, "vars", path)?,
                    file.secrets.unwrap_or_default(),
                ))
            });
        match parsed {
            Ok((vars, secrets)) => Self {
                name: name.to_owned(),
                vars,
                secrets,
                error: None,
            },
            Err(err) => broken(err),
        }
    }

    /// The name, from the file name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Variables in file order.
    #[must_use]
    pub fn vars(&self) -> &[(String, String)] {
        &self.vars
    }

    /// The names of the secrets it declares.
    #[must_use]
    pub fn secrets(&self) -> &[String] {
        &self.secrets
    }

    /// Why the file is not usable.
    #[must_use]
    pub fn error(&self) -> Option<&ModelError> {
        self.error.as_ref()
    }
}
