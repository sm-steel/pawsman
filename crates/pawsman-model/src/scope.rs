//! The scopes a variable is looked up in, highest first, and the built-ins.

use std::fmt;
use std::path::PathBuf;
use std::time::SystemTime;

use secrecy::SecretString;

/// Where a variable's value came from.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Source {
    /// Captures and script assignments (M2).
    Runtime,
    /// The request's `[vars]`.
    Request,
    /// A folder's `_folder.toml`.
    Folder(PathBuf),
    /// The named environment.
    Environment(String),
    /// `pawsman.toml`.
    Collection,
    /// `globals.toml`.
    Global,
    /// The process environment, through `{{env.NAME}}`.
    Process,
    /// A `{{$…}}` built-in.
    Builtin,
}

impl fmt::Display for Source {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Runtime => f.write_str("runtime"),
            Self::Request => f.write_str("request"),
            Self::Folder(path) => write!(f, "folder {}", path.display()),
            Self::Environment(name) => write!(f, "env {name}"),
            Self::Collection => f.write_str("collection"),
            Self::Global => f.write_str("global"),
            Self::Process => f.write_str("process env"),
            Self::Builtin => f.write_str("built-in"),
        }
    }
}

/// How `{{$…}}` built-ins get their values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Builtins(pub(crate) BuiltinMode);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BuiltinMode {
    Live { now: SystemTime, seed: u64 },
    Preview,
}

impl Builtins {
    /// Real values: the time is `now`, random values come from `seed`.
    #[must_use]
    pub fn live(now: SystemTime, seed: u64) -> Self {
        Self(BuiltinMode::Live { now, seed })
    }

    /// Placeholders such as `‹uuid›`, for showing a request before it is sent.
    #[must_use]
    pub fn preview() -> Self {
        Self(BuiltinMode::Preview)
    }
}

/// A variable's value at one scope.
#[derive(Debug, Clone)]
pub(crate) enum Binding {
    Plain(String),
    Secret(Option<SecretString>),
}

/// A process-environment lookup.
pub type ProcessLookup = Box<dyn Fn(&str) -> Option<String> + Send + Sync>;

/// Every scope of one interpolation, highest first.
pub struct Scopes {
    pub(crate) vars: Vec<(Source, String, Binding)>,
    pub(crate) process: Option<ProcessLookup>,
    pub(crate) builtins: Builtins,
}

impl fmt::Debug for Scopes {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Scopes")
            .field("vars", &self.vars)
            .field("process", &self.process.is_some())
            .field("builtins", &self.builtins)
            .finish()
    }
}

impl Scopes {
    /// No variables yet.
    #[must_use]
    pub fn new(builtins: Builtins) -> Self {
        Self {
            vars: Vec::new(),
            process: None,
            builtins,
        }
    }

    /// Adds `vars` at `source`, below every scope added before.
    #[must_use]
    pub fn layer(mut self, source: &Source, vars: &[(String, String)]) -> Self {
        self.vars.extend(
            vars.iter()
                .map(|(k, v)| (source.clone(), k.clone(), Binding::Plain(v.clone()))),
        );
        self
    }

    /// Adds secret `name` at `source`; `None` means declared but not set.
    #[must_use]
    pub fn secret(mut self, source: &Source, name: &str, value: Option<SecretString>) -> Self {
        self.vars
            .push((source.clone(), name.to_owned(), Binding::Secret(value)));
        self
    }

    /// Sets the lookup `{{env.NAME}}` reads.
    #[must_use]
    pub fn process(mut self, lookup: ProcessLookup) -> Self {
        self.process = Some(lookup);
        self
    }

    pub(crate) fn find(&self, name: &str) -> Option<(&Source, &Binding)> {
        self.vars
            .iter()
            .find(|(_, n, _)| n == name)
            .map(|(source, _, binding)| (source, binding))
    }
}
