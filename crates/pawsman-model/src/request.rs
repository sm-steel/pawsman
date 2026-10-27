//! One request file (`*.req.toml`).

mod body;

use std::fmt;
use std::path::Path;

use serde::Deserialize;

use body::BodyFile;
pub use body::{Body, Content, Part, PartValue};
pub(crate) use body::{PartData, Sidecars};

use crate::ModelError;
use crate::manifest::strings;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RequestFile {
    name: String,
    kind: Option<String>,
    method: Option<String>,
    url: String,
    docs: Option<String>,
    headers: Option<toml::Table>,
    query: Option<toml::Table>,
    vars: Option<toml::Table>,
    body: Option<BodyFile>,
    auth: Option<toml::Table>,
    capture: Option<Vec<toml::Table>>,
    assert: Option<Vec<toml::Table>>,
    script: Option<ScriptFile>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ScriptFile {
    pre: Option<String>,
    post: Option<String>,
}

/// A request's id within its collection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RequestId(u32);

impl RequestId {
    /// An id.
    #[must_use]
    pub const fn new(id: u32) -> Self {
        Self(id)
    }
}

impl fmt::Display for RequestId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "#{}", self.0)
    }
}

/// `[query]`: each value a string or an array of strings.
fn query(
    table: Option<toml::Table>,
    path: &Path,
) -> Result<Vec<(String, Vec<String>)>, ModelError> {
    let bad = |key: &str| {
        ModelError::invalid_file(
            path,
            &format!("`query.{key}` must be a string or an array of strings"),
        )
    };
    table
        .unwrap_or_default()
        .into_iter()
        .map(|(key, value)| {
            let values = match value {
                toml::Value::String(v) => vec![v],
                toml::Value::Array(items) => items
                    .into_iter()
                    .map(|item| match item {
                        toml::Value::String(v) => Ok(v),
                        _ => Err(bad(&key)),
                    })
                    .collect::<Result<_, _>>()?,
                _ => return Err(bad(&key)),
            };
            Ok((key, values))
        })
        .collect()
}

/// A parsed request.
#[derive(Debug, Clone, PartialEq)]
pub struct Request {
    pub(crate) name: String,
    pub(crate) method: String,
    pub(crate) url: String,
    pub(crate) docs: Option<String>,
    pub(crate) headers: Vec<(String, String)>,
    pub(crate) query: Vec<(String, Vec<String>)>,
    pub(crate) vars: Vec<(String, String)>,
    pub(crate) body: Option<Body>,
    pub(crate) auth: Option<toml::Table>,
    pub(crate) captures: Vec<toml::Table>,
    pub(crate) asserts: Vec<toml::Table>,
    pub(crate) script: Option<Script>,
}

/// The `[script]` table, kept for M2.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Script {
    pub(crate) pre: Option<String>,
    pub(crate) post: Option<String>,
}

impl Script {
    /// The pre-request script.
    #[must_use]
    pub fn pre(&self) -> Option<&str> {
        self.pre.as_deref()
    }

    /// The post-response script.
    #[must_use]
    pub fn post(&self) -> Option<&str> {
        self.post.as_deref()
    }
}

impl Request {
    /// Parses a request file; `path` names it in errors.
    ///
    /// # Errors
    ///
    /// `model::invalid_file` for bad TOML, unknown keys, a kind other than `http`, or a header
    /// value that is not a string.
    pub fn parse(text: &str, path: &Path, sidecar: Sidecars<'_>) -> Result<Self, ModelError> {
        let file: RequestFile =
            toml::from_str(text).map_err(|e| ModelError::invalid_file(path, e.message()))?;
        if let Some(kind) = file.kind.as_deref()
            && kind != "http"
        {
            return Err(ModelError::invalid_file(
                path,
                &format!("kind `{kind}` is not supported yet; only `http`"),
            ));
        }
        Ok(Self {
            name: file.name,
            method: file
                .method
                .map_or_else(|| "GET".to_owned(), |m| m.to_ascii_uppercase()),
            url: file.url,
            docs: file.docs,
            headers: strings(file.headers, "headers", path)?,
            query: query(file.query, path)?,
            vars: strings(file.vars, "vars", path)?,
            body: file.body.map(|b| b.into_body(path, sidecar)).transpose()?,
            auth: file.auth,
            captures: file.capture.unwrap_or_default(),
            asserts: file.assert.unwrap_or_default(),
            script: file.script.map(|s| Script {
                pre: s.pre,
                post: s.post,
            }),
        })
    }

    /// The display name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The HTTP method, uppercase.
    #[must_use]
    pub fn method(&self) -> &str {
        &self.method
    }

    /// The URL as written.
    #[must_use]
    pub fn url(&self) -> &str {
        &self.url
    }

    /// Headers in file order.
    #[must_use]
    pub fn headers(&self) -> &[(String, String)] {
        &self.headers
    }

    /// The body.
    #[must_use]
    pub fn body(&self) -> Option<&Body> {
        self.body.as_ref()
    }

    /// The text of a UTF-8 JSON or text body.
    #[must_use]
    pub fn body_text(&self) -> Option<&str> {
        match &self.body {
            Some(Body::Json(Content::Utf8(text)) | Body::Text(Content::Utf8(text))) => Some(text),
            _ => None,
        }
    }

    /// Query parameters in file order; a repeated key holds several values.
    #[must_use]
    pub fn query(&self) -> &[(String, Vec<String>)] {
        &self.query
    }

    /// Request variables, in file order.
    #[must_use]
    pub fn vars(&self) -> &[(String, String)] {
        &self.vars
    }

    /// The markdown docs.
    #[must_use]
    pub fn docs(&self) -> Option<&str> {
        self.docs.as_deref()
    }

    /// The `[auth]` table, applied from M3.
    #[must_use]
    pub fn auth(&self) -> Option<&toml::Table> {
        self.auth.as_ref()
    }

    /// The `[[capture]]` tables, run from M2.
    #[must_use]
    pub fn captures(&self) -> &[toml::Table] {
        &self.captures
    }

    /// The `[[assert]]` tables, run from M2.
    #[must_use]
    pub fn asserts(&self) -> &[toml::Table] {
        &self.asserts
    }

    /// The `[script]` table, run from M2.
    #[must_use]
    pub fn script(&self) -> Option<&Script> {
        self.script.as_ref()
    }

    /// Whether captures, asserts or scripts are present; they do nothing before M2.
    #[must_use]
    pub fn has_m2_parts(&self) -> bool {
        !self.captures.is_empty() || !self.asserts.is_empty() || self.script.is_some()
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use pawsman_error::ErrorInfo as _;

    use super::*;

    fn parse(text: &str, path: &str) -> Result<Request, ModelError> {
        Request::parse(text, Path::new(path), &|_| None)
    }

    fn parse_with(text: &str, files: &[(&str, &[u8])]) -> Result<Request, ModelError> {
        let lookup = |name: &str| {
            files
                .iter()
                .find(|(n, _)| *n == name)
                .map(|(_, b)| b.to_vec())
        };
        Request::parse(text, Path::new("/c/r.req.toml"), &lookup)
    }

    const EVERYTHING: &str = r#"name = "Create user"
kind = "http"
method = "post"
url = "{{base}}/users"
docs = "Creates a user."

[headers]
Accept = "application/json"

[query]
page = "1"
ids = ["1", "2"]

[vars]
id = "42"

[body]
json = """{"name": "ada"}"""

[auth]
type = "bearer"

[[capture]]
var = "etag"
from = "header:ETag"

[[assert]]
expr = "status == 200"

[script]
pre = "a"
post = "b"
"#;

    #[test]
    fn every_section_parses() {
        let r = parse(EVERYTHING, "c.req.toml").unwrap();

        assert_eq!(
            (r.method(), r.docs()),
            ("POST", Some("Creates a user.")),
            "top"
        );
        assert_eq!(
            r.query(),
            &[
                ("page".into(), vec!["1".into()]),
                ("ids".into(), vec!["1".into(), "2".into()])
            ],
            "query"
        );
        assert_eq!(r.vars(), &[("id".into(), "42".into())], "vars");
        assert_eq!(
            r.body(),
            Some(&Body::Json(Content::Utf8("{\"name\": \"ada\"}".into()))),
            "json body"
        );
        assert_eq!(
            r.auth()
                .and_then(|a| a.get("type"))
                .and_then(toml::Value::as_str),
            Some("bearer"),
            "auth kept"
        );
        assert_eq!(
            (r.captures().len(), r.asserts().len()),
            (1, 1),
            "m2 tables kept"
        );
        assert_eq!(
            r.script().map(|s| (s.pre(), s.post())),
            Some((Some("a"), Some("b"))),
            "script"
        );
        assert!(r.has_m2_parts(), "m2 parts");
    }

    #[test]
    fn form_multipart_and_file_bodies() {
        let form = parse(
            "name = \"f\"\nurl = \"u\"\n[body.form]\nb = \"2\"\na = \"1\"\n",
            "f",
        )
        .unwrap();
        let multi = parse(
            "name = \"m\"\nurl = \"u\"\n[[body.multipart]]\nname = \"t\"\nvalue = \"v\"\n[[body.multipart]]\nname = \"f\"\nfile = \"a.png\"\ncontent_type = \"image/png\"\n",
            "m",
        )
        .unwrap();
        let file = parse(
            "name = \"x\"\nurl = \"u\"\n[body]\nfile = \"up.bin\"\n",
            "x",
        )
        .unwrap();

        assert_eq!(
            form.body(),
            Some(&Body::Form(vec![
                ("b".into(), "2".into()),
                ("a".into(), "1".into())
            ])),
            "form in order"
        );
        let Some(Body::Multipart(parts)) = multi.body() else {
            panic!("multipart: {:?}", multi.body());
        };
        let values: Vec<_> = parts
            .iter()
            .map(|p| (p.name(), p.value(), p.content_type()))
            .collect();
        assert_eq!(
            values,
            [
                ("t", PartValue::Text("v"), None),
                ("f", PartValue::File(Path::new("a.png")), Some("image/png"))
            ],
            "parts"
        );
        assert_eq!(file.body(), Some(&Body::File("up.bin".into())), "file body");
        assert!(!file.has_m2_parts(), "no m2 parts");
    }

    #[test]
    fn sidecars_resolve_and_binary_stays_binary() {
        let text = parse_with(
            "name = \"s\"\nurl = \"u\"\n[body]\ntext = { sidecar = \"s.body.txt\" }\n",
            &[("s.body.txt", b"hello")],
        )
        .unwrap();
        let binary = parse_with(
            "name = \"s\"\nurl = \"u\"\n[body]\ntext = { sidecar = \"s.body.bin\" }\n",
            &[("s.body.bin", &[0xff, 0x00, 0xfe])],
        )
        .unwrap();

        assert_eq!(text.body_text(), Some("hello"), "utf-8 sidecar");
        assert_eq!(
            binary.body(),
            Some(&Body::Text(Content::Binary(vec![0xff, 0x00, 0xfe]))),
            "binary sidecar"
        );
    }

    #[test]
    fn a_missing_sidecar_is_missing_sidecar() {
        let err = parse_with(
            "name = \"s\"\nurl = \"u\"\n[body]\njson = { sidecar = \"gone.body.json\" }\n",
            &[],
        )
        .unwrap_err();

        assert_eq!(err.code(), crate::codes::MISSING_SIDECAR, "code");
        assert!(err.to_string().contains("gone.body.json"), "{err}");
    }

    #[rstest::rstest]
    #[case::two_bodies("[body]\ntext = \"a\"\njson = \"{}\"\n")]
    #[case::empty_body("[body]\n")]
    #[case::part_both("[[body.multipart]]\nname = \"p\"\nvalue = \"v\"\nfile = \"f\"\n")]
    #[case::part_neither("[[body.multipart]]\nname = \"p\"\n")]
    #[case::sidecar_escape("[body]\ntext = { sidecar = \"../x\" }\n")]
    #[case::sidecar_extra_key("[body]\ntext = { sidecar = \"x\", y = 1 }\n")]
    #[case::script_unknown("[script]\nmid = \"x\"\n")]
    #[case::query_number("[query]\npage = 1\n")]
    #[case::var_number("[vars]\nport = 1\n")]
    #[case::absolute_file("[body]\nfile = \"/home/u/.ssh/id_ed25519\"\n")]
    #[case::parent_file("[body]\nfile = \"../../secret\"\n")]
    #[case::absolute_part("[[body.multipart]]\nname = \"p\"\nfile = \"/etc/passwd\"\n")]
    #[case::parent_part("[[body.multipart]]\nname = \"p\"\nfile = \"a/../../b\"\n")]
    #[case::part_type_crlf(
        "[[body.multipart]]\nname = \"p\"\nvalue = \"v\"\ncontent_type = \"a/b\\r\\nX: y\"\n"
    )]
    fn bad_sections_are_invalid_files(#[case] tail: &str) {
        let err = parse_with(
            &format!("name = \"x\"\nurl = \"u\"\n{tail}"),
            &[("x", b"x")],
        )
        .unwrap_err();

        assert_eq!(err.code(), crate::codes::INVALID_FILE, "{tail:?}: {err}");
    }

    const FULL: &str = "name = \"List users\"\nmethod = \"POST\"\nurl = \"http://h/users\"\n\n[headers]\nB = \"2\"\nA = \"1\"\n\n[body]\ntext = \"{}\"\n";

    #[test]
    fn a_full_request_parses_with_headers_in_file_order() {
        let r = parse(FULL, "u.req.toml").unwrap();

        assert_eq!(
            (r.name(), r.method(), r.url()),
            ("List users", "POST", "http://h/users"),
            "fields"
        );
        assert_eq!(
            r.headers(),
            &[("B".into(), "2".into()), ("A".into(), "1".into())],
            "file order"
        );
        assert_eq!(r.body_text(), Some("{}"), "text body");
    }

    #[test]
    fn method_defaults_to_get_and_is_uppercased() {
        let r = parse(
            "name = \"x\"\nmethod = \"patch\"\nurl = \"http://h\"\n",
            "x",
        )
        .unwrap();
        let get = parse("name = \"x\"\nurl = \"http://h\"\n", "x").unwrap();

        assert_eq!(r.method(), "PATCH", "uppercased");
        assert_eq!(get.method(), "GET", "default");
        assert_eq!(get.body(), None, "no body");
    }

    #[test]
    fn unknown_keys_other_kinds_and_bad_values_are_invalid_files() {
        for text in [
            "name = \"x\"\nurl = \"u\"\nurll = \"u\"\n",
            "name = \"x\"\nurl = \"u\"\nkind = \"grpc\"\n",
            "name = \"x\"\nurl = \"u\"\n[headers]\nA = 1\n",
            "name = ",
        ] {
            let err = parse(text, "bad.req.toml").unwrap_err();
            assert_eq!(err.code(), crate::codes::INVALID_FILE, "{text:?}");
            assert!(
                err.to_string().contains("bad.req.toml"),
                "names the file: {err}"
            );
            assert!(!err.to_string().contains('\n'), "one line: {err}");
        }
    }

    #[test]
    fn an_explicit_http_kind_is_fine() {
        let r = parse("name = \"x\"\nkind = \"http\"\nurl = \"u\"\n", "x");

        assert!(r.is_ok(), "{r:?}");
    }
}
