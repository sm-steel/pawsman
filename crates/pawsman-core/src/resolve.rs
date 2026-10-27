//! Building a request's scopes from the collection, the active environment, the global file and
//! the process environment, and resolving it for sending or for showing.

use std::time::SystemTime;

use pawsman_model::scope::{Builtins, Scopes};
use pawsman_model::trace::Trace;
use pawsman_model::{Collection, ModelError, RequestId, ResolvedRequest};

use crate::env::EnvSnapshot;

/// What resolution needs beyond the collection: the global variables and the process env.
#[derive(Debug, Clone, Default)]
pub struct Context {
    globals: Vec<(String, String)>,
    env: EnvSnapshot,
}

impl Context {
    /// The global variables and the env snapshot.
    #[must_use]
    pub fn new(globals: Vec<(String, String)>, env: EnvSnapshot) -> Self {
        Self { globals, env }
    }
}

/// A request as it will be sent, secrets masked, for the request pane.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Preview {
    method: String,
    url: String,
    headers: Vec<(String, String)>,
    body: Option<String>,
    trace: Trace,
    m2_parts: bool,
    env: Option<String>,
}

impl Preview {
    /// The method.
    #[must_use]
    pub fn method(&self) -> &str {
        &self.method
    }

    /// The URL with its query, secrets masked.
    #[must_use]
    pub fn url(&self) -> &str {
        &self.url
    }

    /// The merged headers, secrets masked.
    #[must_use]
    pub fn headers(&self) -> &[(String, String)] {
        &self.headers
    }

    /// The body as text, secrets masked; files and binary bodies are described.
    #[must_use]
    pub fn body_text(&self) -> Option<&str> {
        self.body.as_deref()
    }

    /// Where each variable came from.
    #[must_use]
    pub fn trace(&self) -> &Trace {
        &self.trace
    }

    /// The request has captures, asserts or scripts, which run from M2.
    #[must_use]
    pub fn has_m2_parts(&self) -> bool {
        self.m2_parts
    }

    /// The environment it was resolved in.
    #[must_use]
    pub fn env(&self) -> Option<&str> {
        self.env.as_deref()
    }
}

/// The scopes of request `id` in environment `env`, highest first.
pub(crate) fn scopes(
    collection: &Collection,
    id: RequestId,
    env: Option<&str>,
    context: &Context,
    builtins: Builtins,
) -> Scopes {
    let _ = (collection, id, env, context);
    Scopes::new(builtins)
}

/// Resolves request `id` for sending at `at` with random values from `seed`.
pub(crate) fn resolve(
    collection: &Collection,
    id: RequestId,
    env: Option<&str>,
    context: &Context,
    at: SystemTime,
    seed: u64,
) -> Result<(ResolvedRequest, Trace), ModelError> {
    let _ = (collection, id, env, context, at, seed, scopes);
    Err(ModelError::unknown_var("todo", "todo"))
}

/// Request `id` resolved for showing: built-ins as placeholders, secrets masked.
pub(crate) fn preview(
    collection: &Collection,
    id: RequestId,
    env: Option<&str>,
    context: &Context,
) -> Option<Result<Preview, ModelError>> {
    let _ = (collection, id, env, context);
    None
}
