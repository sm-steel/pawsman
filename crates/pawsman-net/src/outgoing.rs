//! A request ready to send: method, URL, headers and body, already validated.

use std::borrow::Cow;

use bytes::Bytes;
use http::{HeaderMap, HeaderName, HeaderValue, Method, Uri};
use pawsman_model::Redactor;

use crate::NetError;

/// A validated request.
#[derive(Debug, Clone)]
pub struct Outgoing {
    method: Method,
    uri: Uri,
    headers: HeaderMap,
    body: Bytes,
    redactor: Redactor,
}

impl Outgoing {
    /// Validates the parts of a request; `redactor` hides its secrets in logs and errors.
    ///
    /// # Errors
    ///
    /// `net::invalid_request` naming the bad method, URL, header name or header value.
    pub fn new(
        method: &str,
        url: &str,
        headers: &[(String, String)],
        body: Bytes,
        redactor: Redactor,
    ) -> Result<Self, NetError> {
        match parts(method, url, headers) {
            Ok((method, uri, headers)) => Ok(Self {
                method,
                uri,
                headers,
                body,
                redactor,
            }),
            Err(err) => Err(err.redacted(&redactor)),
        }
    }

    /// The host for logs, secrets hidden.
    #[must_use]
    pub fn log_host(&self) -> Cow<'_, str> {
        self.redactor.redact(self.host())
    }

    /// The path for logs, without the query, secrets hidden.
    #[must_use]
    pub fn log_path(&self) -> Cow<'_, str> {
        self.redactor.redact(self.path())
    }

    /// Hides this request's secrets.
    pub(crate) fn redactor(&self) -> &Redactor {
        &self.redactor
    }

    /// The host without IPv6 brackets, for resolving and TLS.
    #[must_use]
    pub fn host(&self) -> &str {
        let host = self.uri.host().unwrap_or("");
        host.strip_prefix('[')
            .and_then(|h| h.strip_suffix(']'))
            .unwrap_or(host)
    }

    /// The path without the query, for logs.
    #[must_use]
    pub fn path(&self) -> &str {
        self.uri.path()
    }

    /// The method.
    #[must_use]
    pub fn method(&self) -> &Method {
        &self.method
    }

    /// The URL, for other crates' tests.
    #[cfg(feature = "test-util")]
    #[must_use]
    pub fn uri_for_tests(&self) -> String {
        self.uri.to_string()
    }

    /// The first value of header `name`, for other crates' tests.
    #[cfg(feature = "test-util")]
    #[must_use]
    pub fn header_for_tests(&self, name: &str) -> Option<String> {
        self.headers
            .get(name)
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned)
    }

    pub(crate) fn uri(&self) -> &Uri {
        &self.uri
    }

    pub(crate) fn headers(&self) -> &HeaderMap {
        &self.headers
    }

    pub(crate) fn body(&self) -> &Bytes {
        &self.body
    }
}

/// Validates the method, URL and headers.
fn parts(
    method: &str,
    url: &str,
    headers: &[(String, String)],
) -> Result<(Method, Uri, HeaderMap), NetError> {
    let method = Method::from_bytes(method.as_bytes())
        .map_err(|_| NetError::invalid_request("method", method))?;
    let bad_url = || NetError::invalid_request("url", url);
    let uri: Uri = url.parse().map_err(|_| bad_url())?;
    let scheme_ok = matches!(uri.scheme_str(), Some("http" | "https"));
    // Credentials in the URL would land in the Host header and in access logs; auth comes
    // with M3.
    let userinfo = uri.authority().is_some_and(|a| a.as_str().contains('@'));
    if !scheme_ok || userinfo || uri.host().is_none_or(str::is_empty) {
        return Err(bad_url());
    }
    let mut map = HeaderMap::new();
    for (name, value) in headers {
        let name = HeaderName::from_bytes(name.as_bytes())
            .map_err(|_| NetError::invalid_request("header name", name))?;
        let value = HeaderValue::from_str(value)
            .map_err(|_| NetError::invalid_request("header value", value))?;
        map.append(name, value);
    }
    Ok((method, uri, map))
}

#[cfg(test)]
mod tests {
    use pawsman_error::ErrorInfo as _;
    use rstest::rstest;

    use super::*;

    #[rstest]
    #[case::no_scheme("example.com/x")]
    #[case::ftp("ftp://example.com/x")]
    #[case::no_host("http:///x")]
    #[case::spaces("http://exa mple.com")]
    #[case::empty("")]
    #[case::userinfo("http://user:pass@api.example.com/x")]
    fn bad_urls_are_invalid_requests(#[case] url: &str) {
        let err = Outgoing::new("GET", url, &[], Bytes::new(), Redactor::default()).unwrap_err();

        assert_eq!(err.code(), crate::codes::INVALID_REQUEST, "{url}");
        assert!(
            err.to_string().contains(&format!("`{url}`")),
            "names the url: {err}"
        );
    }

    #[test]
    fn bad_methods_and_headers_are_invalid_requests() {
        let method =
            Outgoing::new("GE T", "http://h/", &[], Bytes::new(), Redactor::default()).unwrap_err();
        let header = Outgoing::new(
            "GET",
            "http://h/",
            &[("Bad Name".into(), "v".into())],
            Bytes::new(),
            Redactor::default(),
        )
        .unwrap_err();
        let value = Outgoing::new(
            "GET",
            "http://h/",
            &[("X".into(), "a\nb".into())],
            Bytes::new(),
            Redactor::default(),
        )
        .unwrap_err();

        for (err, what) in [
            (method, "method"),
            (header, "header name"),
            (value, "header value"),
        ] {
            assert_eq!(err.code(), crate::codes::INVALID_REQUEST, "{err}");
            assert!(
                err.to_string().starts_with(&format!("invalid {what}")),
                "{err}"
            );
        }
    }

    fn secret(value: &str) -> Redactor {
        Redactor::new(vec![secrecy::SecretString::from(value)])
    }

    #[test]
    fn invalid_values_are_redacted_in_errors() {
        let url = Outgoing::new(
            "GET",
            "http://h/s3 cr3t",
            &[],
            Bytes::new(),
            secret("s3 cr3t"),
        )
        .unwrap_err();
        let header = Outgoing::new(
            "GET",
            "http://h/",
            &[("X".into(), "tok\nen".into())],
            Bytes::new(),
            secret("tok\nen"),
        )
        .unwrap_err();

        assert_eq!(url.to_string(), "invalid url `http://h/***`", "url");
        assert!(!header.to_string().contains("tok"), "header: {header}");
    }

    #[test]
    fn log_host_and_path_are_redacted() {
        let o = Outgoing::new(
            "GET",
            "http://s3cr3t.example/u/s3cr3t?q=s3cr3t",
            &[],
            Bytes::new(),
            secret("s3cr3t"),
        )
        .unwrap();

        assert_eq!(
            (o.log_host(), o.log_path()),
            ("***.example".into(), "/u/***".into())
        );
        assert_eq!(o.host(), "s3cr3t.example", "the real host still resolves");
    }

    #[test]
    fn a_valid_request_keeps_its_parts() {
        let o = Outgoing::new(
            "POST",
            "https://api.example.com:8443/users?token=secret",
            &[("X-A".into(), "1".into()), ("X-A".into(), "2".into())],
            Bytes::from_static(b"{}"),
            Redactor::default(),
        )
        .unwrap();

        assert_eq!(o.method(), Method::POST, "method");
        assert_eq!(
            (o.host(), o.path()),
            ("api.example.com", "/users"),
            "no query in logs"
        );
        assert_eq!(
            o.headers().get_all("x-a").iter().count(),
            2,
            "repeated headers kept"
        );
        assert_eq!(o.body().as_ref(), b"{}", "body");
    }
}
