//! Resolving a request's `{{…}}` templates from its scopes: a pure function returning the request
//! as it goes on the wire and a trace of where each value came from.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use secrecy::{ExposeSecret, SecretString};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use crate::redact::Redactor;
use crate::request::{Body, Content, PartData};
use crate::scope::{Binding, BuiltinMode, Scopes, Source};
use crate::template::{self, Segment, VarName};
use crate::trace::{Field, Trace, TraceEntry, TraceValue};
use crate::{Folder, Manifest, ModelError, Request};

/// A field may not grow past this many bytes.
const MAX_FIELD: usize = 1 << 20;
/// Variables may not nest deeper than this.
const MAX_DEPTH: usize = 64;

/// A request with every template resolved.
#[derive(Debug, Clone)]
pub struct ResolvedRequest {
    method: String,
    url: String,
    headers: Vec<(String, String)>,
    query: Vec<(String, String)>,
    body: Option<ResolvedBody>,
    redactor: Redactor,
    base_dir: PathBuf,
}

/// A resolved body. Not `non_exhaustive`: a new kind must break the encoder's match.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolvedBody {
    /// JSON text.
    Json(String),
    /// Plain text.
    Text(String),
    /// Bytes from a binary sidecar, sent as is.
    Binary(Vec<u8>),
    /// Form fields.
    Form(Vec<(String, String)>),
    /// Multipart parts.
    Multipart(Vec<ResolvedPart>),
    /// A file, relative to the request's folder.
    File(PathBuf),
}

/// A resolved multipart part.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedPart {
    name: String,
    value: PartSource,
    content_type: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum PartSource {
    Text(String),
    File(PathBuf),
}

/// What a resolved part carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResolvedPartValue<'a> {
    /// Text.
    Text(&'a str),
    /// A file, relative to the request's folder.
    File(&'a Path),
}

impl ResolvedPart {
    /// The field name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The value or file.
    #[must_use]
    pub fn value(&self) -> ResolvedPartValue<'_> {
        match &self.value {
            PartSource::Text(text) => ResolvedPartValue::Text(text),
            PartSource::File(path) => ResolvedPartValue::File(path),
        }
    }

    /// The part's own `Content-Type`.
    #[must_use]
    pub fn content_type(&self) -> Option<&str> {
        self.content_type.as_deref()
    }
}

impl ResolvedRequest {
    /// The method.
    #[must_use]
    pub fn method(&self) -> &str {
        &self.method
    }

    /// The URL, without the `[query]` parameters.
    #[must_use]
    pub fn url(&self) -> &str {
        &self.url
    }

    /// Headers: collection, folder and request ones merged.
    #[must_use]
    pub fn headers(&self) -> &[(String, String)] {
        &self.headers
    }

    /// Query parameters, one pair per value.
    #[must_use]
    pub fn query(&self) -> &[(String, String)] {
        &self.query
    }

    /// The body.
    #[must_use]
    pub fn body(&self) -> Option<&ResolvedBody> {
        self.body.as_ref()
    }

    /// Hides the secret values this request used.
    #[must_use]
    pub fn redactor(&self) -> &Redactor {
        &self.redactor
    }

    /// The request file's folder, relative to the collection root.
    #[must_use]
    pub fn base_dir(&self) -> &Path {
        &self.base_dir
    }
}

/// Resolves `request`, in the folders `chain` (root first) of `collection`, against `scopes`.
///
/// # Errors
///
/// `model::unknown_var`, `model::var_cycle`, `model::missing_secret` or `model::bad_template`.
pub fn interpolate(
    request: &Request,
    chain: &[&Folder],
    collection: &Manifest,
    scopes: &Scopes,
) -> Result<(ResolvedRequest, Trace), ModelError> {
    let mut r = Resolver::new(scopes);
    let url = r.field(&Field::Url, request.url())?;
    let mut headers = Vec::new();
    for (name, value) in merged_headers(request, chain, collection) {
        let name_out = r.field(&Field::HeaderName(name.clone()), &name)?;
        let value_out = r.field(&Field::HeaderValue(name), &value)?;
        headers.push((name_out, value_out));
    }
    let mut query = Vec::new();
    for (key, values) in request.query() {
        let key_out = r.field(&Field::QueryKey(key.clone()), key)?;
        for value in values {
            query.push((
                key_out.clone(),
                r.field(&Field::QueryValue(key.clone()), value)?,
            ));
        }
    }
    let body = request.body().map(|b| r.body(b)).transpose()?;
    let base_dir = chain
        .last()
        .map_or_else(PathBuf::new, |f| f.path().to_owned());
    let resolved = ResolvedRequest {
        method: request.method().to_owned(),
        url,
        headers,
        query,
        body,
        redactor: Redactor::new(r.secrets),
        base_dir,
    };
    Ok((resolved, Trace { entries: r.trace }))
}

/// Collection, then folders root first, then request headers; a later header replaces an
/// earlier one of the same name (any case) in place.
fn merged_headers(
    request: &Request,
    chain: &[&Folder],
    collection: &Manifest,
) -> Vec<(String, String)> {
    let mut merged: Vec<(String, String)> = Vec::new();
    let levels = std::iter::once(collection.headers())
        .chain(chain.iter().map(|f| f.headers()))
        .chain(std::iter::once(request.headers()));
    for (name, value) in levels.flatten() {
        match merged
            .iter_mut()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
        {
            Some(slot) => *slot = (name.clone(), value.clone()),
            None => merged.push((name.clone(), value.clone())),
        }
    }
    merged
}

/// A variable's resolved value; `secret` when it holds a secret anywhere inside.
#[derive(Clone)]
struct Value {
    text: String,
    secret: bool,
}

struct Resolver<'a> {
    scopes: &'a Scopes,
    stack: Vec<String>,
    memo: BTreeMap<String, Value>,
    trace: Vec<TraceEntry>,
    secrets: Vec<SecretString>,
    rng: fastrand::Rng,
}

impl<'a> Resolver<'a> {
    fn new(scopes: &'a Scopes) -> Self {
        let seed = match scopes.builtins.0 {
            BuiltinMode::Live { seed, .. } => seed,
            BuiltinMode::Preview => 0,
        };
        Self {
            scopes,
            stack: Vec::new(),
            memo: BTreeMap::new(),
            trace: Vec::new(),
            secrets: Vec::new(),
            rng: fastrand::Rng::with_seed(seed),
        }
    }

    fn field(&mut self, field: &Field, text: &str) -> Result<String, ModelError> {
        self.expand(field, text, 0).map(|v| v.text)
    }

    fn body(&mut self, body: &Body) -> Result<ResolvedBody, ModelError> {
        Ok(match body {
            Body::Json(Content::Utf8(text)) => ResolvedBody::Json(self.field(&Field::Body, text)?),
            Body::Text(Content::Utf8(text)) => ResolvedBody::Text(self.field(&Field::Body, text)?),
            Body::Json(Content::Binary(bytes)) | Body::Text(Content::Binary(bytes)) => {
                ResolvedBody::Binary(bytes.clone())
            }
            Body::Form(pairs) => ResolvedBody::Form(
                pairs
                    .iter()
                    .map(|(k, v)| Ok((self.field(&Field::Body, k)?, self.field(&Field::Body, v)?)))
                    .collect::<Result<_, ModelError>>()?,
            ),
            Body::Multipart(parts) => ResolvedBody::Multipart(
                parts
                    .iter()
                    .map(|p| {
                        let value = match &p.data {
                            PartData::Text(text) => {
                                PartSource::Text(self.field(&Field::Body, text)?)
                            }
                            PartData::File(path) => PartSource::File(path.clone()),
                        };
                        Ok(ResolvedPart {
                            name: p.name.clone(),
                            value,
                            content_type: p.content_type.clone(),
                        })
                    })
                    .collect::<Result<_, ModelError>>()?,
            ),
            Body::File(path) => ResolvedBody::File(path.clone()),
        })
    }

    fn expand(&mut self, field: &Field, text: &str, depth: usize) -> Result<Value, ModelError> {
        let label = field.to_string();
        let parsed = template::parse(text).map_err(|e| ModelError::bad_template(&label, &e))?;
        let mut out = Value {
            text: String::new(),
            secret: false,
        };
        for segment in parsed.segments() {
            match segment {
                Segment::Literal(literal) => out.text.push_str(literal),
                Segment::Var { name, span } => {
                    let slot = self.trace.len();
                    let (value, source) = self.var(*name, &label, depth)?;
                    self.trace.insert(
                        slot,
                        TraceEntry {
                            field: field.clone(),
                            span: span.clone(),
                            name: written(*name),
                            source,
                            value: if value.secret {
                                TraceValue::Secret
                            } else {
                                TraceValue::Plain(value.text.clone())
                            },
                        },
                    );
                    out.text.push_str(&value.text);
                    out.secret |= value.secret;
                }
            }
            if out.text.len() > MAX_FIELD {
                return Err(ModelError::runaway_template(
                    &label,
                    "expands to more than 1 MiB",
                ));
            }
        }
        Ok(out)
    }

    fn var(
        &mut self,
        name: VarName<'_>,
        label: &str,
        depth: usize,
    ) -> Result<(Value, Source), ModelError> {
        match name {
            VarName::Builtin(builtin) => Ok((self.builtin(builtin, label)?, Source::Builtin)),
            VarName::Env(key) => {
                let value = self
                    .scopes
                    .process
                    .as_ref()
                    .and_then(|lookup| lookup(key))
                    .ok_or_else(|| ModelError::unknown_var(&format!("env.{key}"), label))?;
                self.secrets.push(SecretString::from(value.clone()));
                Ok((
                    Value {
                        text: value,
                        secret: true,
                    },
                    Source::Process,
                ))
            }
            VarName::Plain(key) => self.plain(key, label, depth),
        }
    }

    fn plain(
        &mut self,
        key: &str,
        label: &str,
        depth: usize,
    ) -> Result<(Value, Source), ModelError> {
        let (source, binding) = self
            .scopes
            .find(key)
            .ok_or_else(|| ModelError::unknown_var(key, label))?;
        let source = source.clone();
        if let Some(value) = self.memo.get(key) {
            return Ok((value.clone(), source));
        }
        if let Some(start) = self.stack.iter().position(|n| n == key) {
            let mut chain: Vec<String> = self.stack.get(start..).unwrap_or_default().to_vec();
            chain.push(key.to_owned());
            return Err(ModelError::var_cycle(chain));
        }
        if depth >= MAX_DEPTH {
            return Err(ModelError::runaway_template(
                label,
                "nested more than 64 deep",
            ));
        }
        let value = match binding {
            Binding::Plain(text) => {
                let text = text.clone();
                self.stack.push(key.to_owned());
                let value = self.expand(&Field::Var(key.to_owned()), &text, depth + 1);
                self.stack.pop();
                value?
            }
            Binding::Secret(Some(secret)) => {
                self.secrets.push(secret.clone());
                Value {
                    text: secret.expose_secret().to_owned(),
                    secret: true,
                }
            }
            Binding::Secret(None) => {
                let env = match &source {
                    Source::Environment(env) => env.clone(),
                    other => other.to_string(),
                };
                return Err(ModelError::missing_secret(key, &env));
            }
        };
        self.memo.insert(key.to_owned(), value.clone());
        Ok((value, source))
    }

    fn builtin(&mut self, name: &str, label: &str) -> Result<Value, ModelError> {
        let known = matches!(name, "uuid" | "timestamp" | "isoDate" | "randomInt");
        if !known {
            return Err(ModelError::unknown_var(&format!("${name}"), label));
        }
        let text = match self.scopes.builtins.0 {
            BuiltinMode::Preview => format!("‹{name}›"),
            BuiltinMode::Live { now, .. } => match name {
                "uuid" => {
                    let mut bytes = [0u8; 16];
                    self.rng.fill(&mut bytes);
                    uuid::Builder::from_random_bytes(bytes)
                        .into_uuid()
                        .to_string()
                }
                "timestamp" => now
                    .duration_since(UNIX_EPOCH)
                    .map_or(0, |d| d.as_secs())
                    .to_string(),
                "isoDate" => OffsetDateTime::from(now)
                    .replace_nanosecond(0)
                    .ok()
                    .and_then(|t| t.format(&Rfc3339).ok())
                    .unwrap_or_default(),
                _ => self.rng.u32(0..=1000).to_string(),
            },
        };
        Ok(Value {
            text,
            secret: false,
        })
    }
}

/// The name as the user wrote it.
fn written(name: VarName<'_>) -> String {
    match name {
        VarName::Plain(n) => n.to_owned(),
        VarName::Env(n) => format!("env.{n}"),
        VarName::Builtin(n) => format!("${n}"),
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};
    use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

    use pawsman_error::ErrorInfo as _;
    use proptest::prelude::*;
    use secrecy::SecretString;

    use super::*;
    use crate::scope::{Builtins, Source};
    use crate::trace::{Field, TraceValue};
    use crate::{Body, Content, codes};

    fn req(text: &str) -> Request {
        Request::parse(text, Path::new("r.req.toml"), &|_| None).unwrap()
    }

    fn manifest(tail: &str) -> Manifest {
        Manifest::parse(&format!("format = 1\nname = \"c\"\n{tail}"), Path::new("m")).unwrap()
    }

    fn folder(path: &str, headers: &[(&str, &str)], vars: &[(&str, &str)]) -> Folder {
        let own = |v: &[(&str, &str)]| {
            v.iter()
                .map(|(a, b)| ((*a).to_owned(), (*b).to_owned()))
                .collect()
        };
        Folder {
            name: path.to_owned(),
            path: PathBuf::from(path),
            headers: own(headers),
            vars: own(vars),
            auth: None,
            children: Vec::new(),
            error: None,
        }
    }

    fn now() -> SystemTime {
        UNIX_EPOCH + Duration::from_hours(500_000)
    }

    fn scopes() -> Scopes {
        Scopes::new(Builtins::live(now(), 7))
    }

    fn pairs(v: &[(&str, &str)]) -> Vec<(String, String)> {
        v.iter()
            .map(|(a, b)| ((*a).to_owned(), (*b).to_owned()))
            .collect()
    }

    fn run(
        r: &Request,
        chain: &[&Folder],
        m: &Manifest,
        s: &Scopes,
    ) -> Result<(ResolvedRequest, Trace), ModelError> {
        interpolate(r, chain, m, s)
    }

    #[test]
    fn highest_scope_wins() {
        let m = manifest("[vars]\nbase = \"http://collection\"\n");
        let with_request =
            req("name = \"x\"\nurl = \"{{base}}/u\"\n[vars]\nbase = \"http://request\"\n");
        let without = req("name = \"x\"\nurl = \"{{base}}/u\"\n");
        let s = || {
            scopes()
                .layer(&Source::Request, with_request.vars())
                .layer(
                    &Source::Environment("dev".into()),
                    &pairs(&[("base", "http://dev")]),
                )
                .layer(&Source::Collection, m.vars())
        };
        let s_without = scopes()
            .layer(
                &Source::Environment("dev".into()),
                &pairs(&[("base", "http://dev")]),
            )
            .layer(&Source::Collection, m.vars());

        let (r, t) = run(&with_request, &[], &m, &s()).unwrap();
        let (r2, t2) = run(&without, &[], &m, &s_without).unwrap();

        assert_eq!(r.url(), "http://request/u", "request wins");
        assert_eq!(
            t.entries().first().map(|e| e.source().to_string()),
            Some("request".into())
        );
        assert_eq!(r2.url(), "http://dev/u", "env next");
        assert_eq!(
            t2.entries().first().map(|e| e.source().to_string()),
            Some("env dev".into())
        );
    }

    #[test]
    fn folder_chain_nearest_first() {
        let m = manifest("");
        let root = folder("", &[], &[]);
        let outer = folder("users", &[], &[("v", "outer")]);
        let inner = folder("users/admin", &[], &[("v", "inner")]);
        let r = req("name = \"x\"\nurl = \"http://h/{{v}}\"\n");
        let s = scopes()
            .layer(&Source::Folder("users/admin".into()), inner.vars())
            .layer(&Source::Folder("users".into()), outer.vars());

        let (resolved, trace) = run(&r, &[&root, &outer, &inner], &m, &s).unwrap();

        assert_eq!(resolved.url(), "http://h/inner", "nearest wins");
        assert_eq!(
            trace.entries().first().map(|e| e.source().to_string()),
            Some("folder users/admin".into())
        );
        assert_eq!(resolved.base_dir(), Path::new("users/admin"), "base dir");
    }

    #[test]
    fn env_dot_reads_the_process_lookup_only() {
        let m = manifest("");
        let s = || scopes().process(Box::new(|k| (k == "HOME").then(|| "/home/u".to_owned())));

        let (env, trace) = run(
            &req("name = \"x\"\nurl = \"http://h{{env.HOME}}\"\n"),
            &[],
            &m,
            &s(),
        )
        .unwrap();
        let plain = run(
            &req("name = \"x\"\nurl = \"http://h{{HOME}}\"\n"),
            &[],
            &m,
            &s(),
        )
        .unwrap_err();
        let unset = run(
            &req("name = \"x\"\nurl = \"http://h{{env.NOPE}}\"\n"),
            &[],
            &m,
            &s(),
        )
        .unwrap_err();

        assert_eq!(env.url(), "http://h/home/u", "env read");
        assert_eq!(
            trace.entries().first().map(|e| e.value().clone()),
            Some(TraceValue::Secret),
            "masked"
        );
        assert_eq!(env.redactor().redact(env.url()), "http://h***", "redacted");
        assert_eq!(
            plain.code(),
            codes::UNKNOWN_VAR,
            "plain names never read the process"
        );
        assert!(unset.to_string().contains("env.NOPE"), "{unset}");
    }

    #[test]
    fn unknown_var_names_the_variable_and_field() {
        let err = run(
            &req("name = \"x\"\nurl = \"u\"\n[headers]\nX-Id = \"{{id}}\"\n"),
            &[],
            &manifest(""),
            &scopes(),
        )
        .unwrap_err();

        assert_eq!(err.code(), codes::UNKNOWN_VAR, "code");
        assert_eq!(
            err.to_string(),
            "`{{id}}` in headers.X-Id is not defined in any scope"
        );
    }

    #[test]
    fn nested_values_resolve_and_trace_both() {
        let s = scopes().layer(
            &Source::Collection,
            &pairs(&[("api", "{{base}}/v1"), ("base", "http://h")]),
        );

        let (r, t) = run(
            &req("name = \"x\"\nurl = \"{{api}}/u\"\n"),
            &[],
            &manifest(""),
            &s,
        )
        .unwrap();

        assert_eq!(r.url(), "http://h/v1/u", "nested");
        let entries: Vec<_> = t
            .entries()
            .iter()
            .map(|e| (e.field().clone(), e.name().to_owned(), e.span()))
            .collect();
        assert_eq!(
            entries,
            [
                (Field::Url, "api".to_owned(), 0..7),
                (Field::Var("api".into()), "base".to_owned(), 0..8)
            ],
            "parent first, then what it needed"
        );
    }

    #[test]
    fn a_cycle_is_var_cycle() {
        let s = scopes().layer(
            &Source::Collection,
            &pairs(&[("a", "x{{b}}"), ("b", "{{a}}")]),
        );

        let err = run(
            &req("name = \"x\"\nurl = \"{{a}}\"\n"),
            &[],
            &manifest(""),
            &s,
        )
        .unwrap_err();

        assert_eq!(err.code(), codes::VAR_CYCLE, "code");
        assert!(err.to_string().contains("`a` → `b` → `a`"), "{err}");
    }

    #[test]
    fn a_large_literal_body_is_not_a_runaway() {
        let mut r = req("name = \"x\"\nurl = \"u\"\n");
        let big = "x".repeat(3 << 20);
        r.body = Some(Body::Text(Content::Utf8(format!("{big}{{{{v}}}}"))));
        let s = scopes().layer(&Source::Collection, &pairs(&[("v", "1")]));

        let (resolved, _) = run(&r, &[], &manifest(""), &s).unwrap();

        assert_eq!(
            resolved
                .body()
                .map(|b| matches!(b, ResolvedBody::Text(t) if t.len() == big.len() + 1)),
            Some(true),
            "3 MiB of literal text plus one variable"
        );
    }

    #[test]
    fn the_whole_request_has_an_expansion_budget() {
        let mut vars: Vec<(String, String)> = (0..19)
            .map(|n| {
                (
                    format!("b{n}"),
                    format!("{{{{b{}}}}}{{{{b{}}}}}", n + 1, n + 1),
                )
            })
            .collect();
        vars.push(("b19".into(), "x".into()));
        let headers: String = (0..1000)
            .map(|n| format!("X-{n} = \"{{{{b1}}}}\"\n"))
            .collect();
        let r = req(&format!("name = \"x\"\nurl = \"u\"\n[headers]\n{headers}"));
        let s = scopes().layer(&Source::Collection, &vars);
        let started = Instant::now();

        let err = run(&r, &[], &manifest(""), &s).unwrap_err();

        assert_eq!(err.code(), codes::BAD_TEMPLATE, "{err}");
        assert!(err.to_string().contains("in total"), "{err}");
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "{:?}",
            started.elapsed()
        );
    }

    #[test]
    fn trace_values_are_shortened() {
        let s = scopes().layer(&Source::Collection, &pairs(&[("v", &"y".repeat(10_000))]));

        let (_, trace) = run(
            &req("name = \"x\"\nurl = \"{{v}}\"\n"),
            &[],
            &manifest(""),
            &s,
        )
        .unwrap();

        let Some(TraceValue::Plain(value)) = trace.entries().first().map(|e| e.value().clone())
        else {
            panic!("{trace:?}");
        };
        assert!(
            value.chars().count() <= 201 && value.ends_with('…'),
            "{} chars",
            value.len()
        );
    }

    #[test]
    fn expansion_is_capped() {
        let mut vars: Vec<(String, String)> = (0..30)
            .map(|n| {
                (
                    format!("v{n}"),
                    format!("{{{{v{}}}}}{{{{v{}}}}}", n + 1, n + 1),
                )
            })
            .collect();
        vars.push(("v30".into(), "xxxxxxxx".into()));
        let s = scopes().layer(&Source::Collection, &vars);
        let started = Instant::now();

        let err = run(
            &req("name = \"x\"\nurl = \"{{v0}}\"\n"),
            &[],
            &manifest(""),
            &s,
        )
        .unwrap_err();

        assert_eq!(err.code(), codes::BAD_TEMPLATE, "{err}");
        assert!(err.to_string().contains("1 MiB"), "{err}");
        assert!(
            started.elapsed() < Duration::from_secs(1),
            "fast: {:?}",
            started.elapsed()
        );
    }

    #[test]
    fn deep_nesting_is_capped() {
        let mut vars: Vec<(String, String)> = (0..70)
            .map(|n| (format!("v{n}"), format!("{{{{v{}}}}}", n + 1)))
            .collect();
        vars.push(("v70".into(), "end".into()));
        let s = scopes().layer(&Source::Collection, &vars);

        let err = run(
            &req("name = \"x\"\nurl = \"{{v0}}\"\n"),
            &[],
            &manifest(""),
            &s,
        )
        .unwrap_err();

        assert!(err.to_string().contains("64 deep"), "{err}");
    }

    #[test]
    fn a_declared_unset_secret_is_missing_secret() {
        let s = scopes().secret(&Source::Environment("dev".into()), "token", None);

        let err = run(
            &req("name = \"x\"\nurl = \"http://h/{{token}}\"\n"),
            &[],
            &manifest(""),
            &s,
        )
        .unwrap_err();

        assert_eq!(err.code(), codes::MISSING_SECRET, "code");
        assert!(
            err.to_string().contains("env dev") && err.to_string().contains("PAWSMAN_SECRET_TOKEN"),
            "{err}"
        );
    }

    #[test]
    fn secrets_resolve_and_are_secret_in_the_trace() {
        let s = scopes()
            .secret(
                &Source::Environment("dev".into()),
                "token",
                Some(SecretString::from("s3cr3t")),
            )
            .layer(&Source::Collection, &pairs(&[("auth", "Bearer {{token}}")]));
        let r = req(
            "name = \"x\"\nurl = \"http://h/{{token}}\"\n[headers]\nAuthorization = \"{{auth}}\"\n",
        );

        let (resolved, trace) = run(&r, &[], &manifest(""), &s).unwrap();

        assert_eq!(
            resolved.headers(),
            &pairs(&[("Authorization", "Bearer s3cr3t")]),
            "used on the wire"
        );
        assert!(
            trace
                .entries()
                .iter()
                .all(|e| e.value() == &TraceValue::Secret),
            "the secret and the value holding it are both hidden: {trace:?}"
        );
        assert_eq!(
            resolved.redactor().redact(resolved.url()),
            "http://h/***",
            "redacted"
        );
    }

    #[test]
    fn request_headers_override_defaults_case_insensitively() {
        let m = manifest("[headers]\nX-Trace = \"1\"\naccept = \"from collection\"\n");
        let users = folder("users", &[("ACCEPT", "from folder")], &[]);
        let r = req("name = \"x\"\nurl = \"u\"\n[headers]\nAccept = \"b\"\nX-New = \"n\"\n");

        let (resolved, _) = run(&r, &[&users], &m, &scopes()).unwrap();

        assert_eq!(
            resolved.headers(),
            &pairs(&[("X-Trace", "1"), ("Accept", "b"), ("X-New", "n")]),
            "the request's spelling, the first position, sent once"
        );
    }

    #[test]
    fn builtins_are_deterministic_for_a_seed_and_fresh_per_occurrence() {
        let r = req(
            "name = \"x\"\nurl = \"{{$uuid}}|{{$uuid}}|{{$isoDate}}|{{$timestamp}}|{{$randomInt}}\"\n",
        );

        let (a, trace) = run(&r, &[], &manifest(""), &scopes()).unwrap();
        let (b, _) = run(&r, &[], &manifest(""), &scopes()).unwrap();
        let parts: Vec<&str> = a.url().split('|').collect();

        assert_eq!(a.url(), b.url(), "same seed, same values");
        assert_ne!(parts.first(), parts.get(1), "fresh per occurrence");
        assert_eq!(parts.first().map(|u| u.len()), Some(36), "hyphenated uuid");
        assert_eq!(
            parts.first().and_then(|u| u.chars().nth(14)),
            Some('4'),
            "version 4"
        );
        assert_eq!(parts.get(2), Some(&"2027-01-15T08:00:00Z"), "iso date");
        assert_eq!(parts.get(3), Some(&"1800000000"), "timestamp");
        assert!(
            parts
                .get(4)
                .and_then(|n| n.parse::<u32>().ok())
                .is_some_and(|n| n <= 1000),
            "{parts:?}"
        );
        assert_eq!(
            trace.entries().first().map(|e| e.source().clone()),
            Some(Source::Builtin),
            "source"
        );
        let unknown = run(
            &req("name = \"x\"\nurl = \"{{$nope}}\"\n"),
            &[],
            &manifest(""),
            &scopes(),
        )
        .unwrap_err();
        assert_eq!(unknown.code(), codes::UNKNOWN_VAR, "unknown built-in");
    }

    #[test]
    fn preview_builtins_are_placeholders() {
        let r =
            req("name = \"x\"\nurl = \"{{$uuid}}/{{$timestamp}}/{{$isoDate}}/{{$randomInt}}\"\n");

        let (p, _) = run(&r, &[], &manifest(""), &Scopes::new(Builtins::preview())).unwrap();

        assert_eq!(p.url(), "‹uuid›/‹timestamp›/‹isoDate›/‹randomInt›");
    }

    #[test]
    fn escaped_braces_survive_into_the_wire() {
        let (r, _) = run(
            &req("name = \"x\"\nurl = \"http://h/\\\\{{x}}\"\n"),
            &[],
            &manifest(""),
            &scopes(),
        )
        .unwrap();

        assert_eq!(r.url(), "http://h/{{x}}");
    }

    #[test]
    fn query_form_and_multipart_values_are_templated() {
        let s = scopes().layer(&Source::Collection, &pairs(&[("v", "1")]));
        let q = req("name = \"x\"\nurl = \"u\"\n[query]\n\"k{{v}}\" = [\"a{{v}}\", \"b\"]\n");
        let form = req("name = \"x\"\nurl = \"u\"\n[body.form]\nf = \"{{v}}\"\n");
        let multi = req(
            "name = \"x\"\nurl = \"u\"\n[[body.multipart]]\nname = \"p\"\nvalue = \"{{v}}\"\n[[body.multipart]]\nname = \"f\"\nfile = \"{{v}}.png\"\n",
        );

        let (q, _) = run(&q, &[], &manifest(""), &s).unwrap();
        let (form, _) = run(&form, &[], &manifest(""), &s).unwrap();
        let (multi, _) = run(&multi, &[], &manifest(""), &s).unwrap();

        assert_eq!(
            q.query(),
            &pairs(&[("k1", "a1"), ("k1", "b")]),
            "query flattened"
        );
        assert_eq!(
            form.body(),
            Some(&ResolvedBody::Form(pairs(&[("f", "1")]))),
            "form"
        );
        let Some(ResolvedBody::Multipart(parts)) = multi.body() else {
            panic!("{:?}", multi.body());
        };
        let values: Vec<_> = parts.iter().map(ResolvedPart::value).collect();
        assert_eq!(
            values,
            [
                ResolvedPartValue::Text("1"),
                ResolvedPartValue::File(Path::new("{{v}}.png"))
            ],
            "text templated, file paths not"
        );
    }

    #[test]
    fn only_utf8_bodies_are_templated() {
        let mut r = req("name = \"x\"\nurl = \"u\"\n");
        r.body = Some(Body::Text(Content::Binary(b"{{x}}\xff".to_vec())));
        let mut json = req("name = \"x\"\nurl = \"u\"\n");
        json.body = Some(Body::Json(Content::Utf8("{\"a\": \"{{v}}\"}".into())));
        let s = scopes().layer(&Source::Collection, &pairs(&[("v", "1")]));

        let (binary, _) = run(&r, &[], &manifest(""), &s).unwrap();
        let (json, _) = run(&json, &[], &manifest(""), &s).unwrap();

        assert_eq!(
            binary.body(),
            Some(&ResolvedBody::Binary(b"{{x}}\xff".to_vec())),
            "bytes pass"
        );
        assert_eq!(
            json.body(),
            Some(&ResolvedBody::Json("{\"a\": \"1\"}".into())),
            "json templated"
        );
    }

    #[test]
    fn a_bad_template_names_its_field() {
        let err = run(
            &req("name = \"x\"\nurl = \"{{a\"\n"),
            &[],
            &manifest(""),
            &scopes(),
        )
        .unwrap_err();

        assert_eq!(err.code(), codes::BAD_TEMPLATE, "code");
        assert!(
            err.to_string().starts_with("`url` has a bad template at 0"),
            "{err}"
        );
    }

    proptest! {
        #[test]
        fn interpolate_never_panics(url in "[{}a-z$. \\\\]{0,30}") {
            let s = scopes().layer(&Source::Collection, &pairs(&[("a", "{{b}}"), ("b", "x"), ("c", "{{c}}")]));
            let r = req(&format!("name = \"x\"\nurl = {}\n", toml::Value::String(url)));

            let _ = run(&r, &[], &manifest(""), &s);
        }
    }
}
