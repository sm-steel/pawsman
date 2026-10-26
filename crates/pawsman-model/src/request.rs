//! One request file (`*.req.toml`).

use std::fmt;
use std::path::Path;

use crate::ModelError;

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

/// A parsed request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    name: String,
    method: String,
    url: String,
    headers: Vec<(String, String)>,
    body: Option<String>,
}

impl Request {
    /// Parses a request file; `path` names it in errors.
    ///
    /// # Errors
    ///
    /// `model::invalid_file` for bad TOML, unknown keys, a kind other than `http`, or a header
    /// value that is not a string.
    pub fn parse(text: &str, path: &Path) -> Result<Self, ModelError> {
        let _ = text;
        Err(ModelError::invalid_file(path, "todo"))
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

    /// The text body.
    #[must_use]
    pub fn body(&self) -> Option<&str> {
        self.body.as_deref()
    }

    /// Whether any `{{…}}` appears; variables are resolved from piece 1.3 on.
    #[must_use]
    pub fn has_template(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use pawsman_error::ErrorInfo as _;

    use super::*;

    const FULL: &str = "name = \"List users\"\nmethod = \"POST\"\nurl = \"http://h/users\"\n\n[headers]\nB = \"2\"\nA = \"1\"\n\n[body]\ntext = \"{}\"\n";

    #[test]
    fn a_full_request_parses_with_headers_in_file_order() {
        let r = Request::parse(FULL, Path::new("u.req.toml")).unwrap();

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
        assert_eq!(r.body(), Some("{}"), "text body");
    }

    #[test]
    fn method_defaults_to_get_and_is_uppercased() {
        let r = Request::parse(
            "name = \"x\"\nmethod = \"patch\"\nurl = \"http://h\"\n",
            Path::new("x"),
        )
        .unwrap();
        let get = Request::parse("name = \"x\"\nurl = \"http://h\"\n", Path::new("x")).unwrap();

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
            let err = Request::parse(text, Path::new("bad.req.toml")).unwrap_err();
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
        let r = Request::parse(
            "name = \"x\"\nkind = \"http\"\nurl = \"u\"\n",
            Path::new("x"),
        );

        assert!(r.is_ok(), "{r:?}");
    }

    #[test]
    fn templates_are_noticed() {
        let r = Request::parse("name = \"x\"\nurl = \"{{base}}/u\"\n", Path::new("x")).unwrap();
        let plain = Request::parse("name = \"x\"\nurl = \"http://h/u\"\n", Path::new("x")).unwrap();

        assert!(r.has_template(), "an unresolved variable");
        assert!(!plain.has_template(), "none");
    }
}
