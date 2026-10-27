//! Building a request's scopes from the collection, the active environment, the global file and
//! the process environment, and resolving it for sending or for showing.

use std::time::SystemTime;

use pawsman_model::redact::encode_form;
use pawsman_model::scope::{Builtins, Scopes, Source};
use pawsman_model::trace::Trace;
use pawsman_model::{
    Collection, ModelError, RequestId, ResolvedBody, ResolvedPartValue, ResolvedRequest,
    interpolate, secrets,
};

use crate::env::EnvSnapshot;
use crate::wire;

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
    let mut scopes = Scopes::new(builtins);
    let Some(request) = collection.request(id) else {
        return scopes;
    };
    scopes = scopes.layer(&Source::Request, request.vars());
    for folder in collection.chain(id).iter().rev() {
        let source = if folder.path().as_os_str().is_empty() {
            Source::Collection
        } else {
            Source::Folder(folder.path().to_owned())
        };
        scopes = scopes.layer(&source, folder.vars());
    }
    let active = env.and_then(|name| collection.environments().iter().find(|e| e.name() == name));
    if let Some(environment) = active {
        let source = Source::Environment(environment.name().to_owned());
        let lookup = context.env.lookup();
        for name in environment.secrets() {
            let value = secrets::resolve(name, &*lookup, collection.dotenv());
            scopes = scopes.secret(&source, name, value);
        }
        scopes = scopes.layer(&source, environment.vars());
    }
    scopes
        .layer(&Source::Collection, collection.manifest().vars())
        .layer(&Source::Global, &context.globals)
        .process(context.env.lookup())
}

/// Resolves request `id` for sending at `at` with random values from `seed`; `None` when `id` is
/// not a parsed request.
pub(crate) fn resolve(
    collection: &Collection,
    id: RequestId,
    env: Option<&str>,
    context: &Context,
    at: SystemTime,
    seed: u64,
) -> Option<Result<(ResolvedRequest, Trace), ModelError>> {
    let request = collection.request(id)?;
    let scopes = scopes(collection, id, env, context, Builtins::live(at, seed));
    Some(interpolate(
        request,
        &collection.chain(id),
        collection.manifest(),
        &scopes,
    ))
}

/// Request `id` resolved for showing: built-ins as placeholders, secrets masked.
pub(crate) fn preview(
    collection: &Collection,
    id: RequestId,
    env: Option<&str>,
    context: &Context,
) -> Option<Result<Preview, ModelError>> {
    let request = collection.request(id)?;
    let scopes = scopes(collection, id, env, context, Builtins::preview());
    let resolved = interpolate(
        request,
        &collection.chain(id),
        collection.manifest(),
        &scopes,
    );
    Some(resolved.map(|(r, trace)| {
        let mask = |text: &str| r.redactor().mask(text).into_owned();
        Preview {
            method: r.method().to_owned(),
            url: mask(&wire::url_with_query(r.url(), r.query())),
            headers: r
                .headers()
                .iter()
                .map(|(name, value)| (mask(name), mask(value)))
                .collect(),
            body: r.body().map(|body| mask(&describe(body))),
            trace,
            m2_parts: request.has_m2_parts(),
            env: env.map(str::to_owned),
        }
    }))
}

/// A body as text for the request pane.
fn describe(body: &ResolvedBody) -> String {
    match body {
        ResolvedBody::Json(text) | ResolvedBody::Text(text) => text.clone(),
        ResolvedBody::Binary(bytes) => format!("‹{} bytes›", bytes.len()),
        ResolvedBody::Form(pairs) => pairs
            .iter()
            .map(|(k, v)| format!("{}={}", encode_form(k), encode_form(v)))
            .collect::<Vec<_>>()
            .join("&"),
        ResolvedBody::Multipart(parts) => parts
            .iter()
            .map(|p| match p.value() {
                ResolvedPartValue::Text(text) => format!("{}: {text}", p.name()),
                ResolvedPartValue::File(path) => format!("{}: @{}", p.name(), path.display()),
            })
            .collect::<Vec<_>>()
            .join("\n"),
        ResolvedBody::File(path) => format!("@{}", path.display()),
    }
}
