//! A request ready to send: method, URL, headers and body, already validated.

use bytes::Bytes;
use http::{HeaderMap, HeaderName, HeaderValue, Method, Uri};

use crate::NetError;

/// A validated request.
#[derive(Debug, Clone)]
pub struct Outgoing {
    method: Method,
    uri: Uri,
    headers: HeaderMap,
    body: Bytes,
}

impl Outgoing {
    /// Validates the parts of a request.
    ///
    /// # Errors
    ///
    /// `net::invalid_request` naming the bad method, URL, header name or header value.
    pub fn new(
        method: &str,
        url: &str,
        headers: &[(String, String)],
        body: Option<&str>,
    ) -> Result<Self, NetError> {
        let method = Method::from_bytes(method.as_bytes())
            .map_err(|_| NetError::invalid_request("method", method))?;
        let bad_url = || NetError::invalid_request("url", url);
        let uri: Uri = url.parse().map_err(|_| bad_url())?;
        let scheme_ok = matches!(uri.scheme_str(), Some("http" | "https"));
        if !scheme_ok || uri.host().is_none_or(str::is_empty) {
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
        Ok(Self {
            method,
            uri,
            headers: map,
            body: body.map_or_else(Bytes::new, |b| Bytes::copy_from_slice(b.as_bytes())),
        })
    }

    /// The host, for logs.
    #[must_use]
    pub fn host(&self) -> &str {
        self.uri.host().unwrap_or("")
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
        let err = Outgoing::new("GET", url, &[], None).unwrap_err();

        assert_eq!(err.code(), crate::codes::INVALID_REQUEST, "{url}");
        assert!(
            err.to_string().contains(&format!("`{url}`")),
            "names the url: {err}"
        );
    }

    #[test]
    fn bad_methods_and_headers_are_invalid_requests() {
        let method = Outgoing::new("GE T", "http://h/", &[], None).unwrap_err();
        let header = Outgoing::new("GET", "http://h/", &[("Bad Name".into(), "v".into())], None)
            .unwrap_err();
        let value =
            Outgoing::new("GET", "http://h/", &[("X".into(), "a\nb".into())], None).unwrap_err();

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

    #[test]
    fn a_valid_request_keeps_its_parts() {
        let o = Outgoing::new(
            "POST",
            "https://api.example.com:8443/users?token=secret",
            &[("X-A".into(), "1".into()), ("X-A".into(), "2".into())],
            Some("{}"),
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
