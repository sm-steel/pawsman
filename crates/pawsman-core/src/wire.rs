//! Turning a resolved request into what goes on the wire: the URL with its query, the headers
//! with a default `Content-Type`, and the body bytes, reading body files from disk.

use std::path::Path;

use bytes::Bytes;
use pawsman_model::ResolvedRequest;

use crate::CoreError;

/// A request ready for `Outgoing::new`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Wire {
    /// The URL with the query appended.
    pub url: String,
    /// The headers, a default `Content-Type` added.
    pub headers: Vec<(String, String)>,
    /// The body bytes; empty for none.
    pub body: Bytes,
}

/// Encodes `resolved`; body files are read relative to its folder under `root`. The multipart
/// boundary comes from `seed`.
///
/// # Errors
///
/// `core::body_file` when a body file cannot be read.
pub fn encode(resolved: &ResolvedRequest, root: &Path, seed: u64) -> Result<Wire, CoreError> {
    let _ = (root, seed);
    Ok(Wire {
        url: resolved.url().to_owned(),
        headers: resolved.headers().to_vec(),
        body: Bytes::new(),
    })
}

/// `url` with `query` appended, before any `#fragment`.
#[must_use]
pub fn url_with_query(url: &str, query: &[(String, String)]) -> String {
    let _ = query;
    url.to_owned()
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::time::UNIX_EPOCH;

    use pawsman_error::ErrorInfo as _;
    use pawsman_model::scope::{Builtins, Scopes};
    use pawsman_model::{Manifest, Request, interpolate};

    use super::*;

    fn resolve(text: &str) -> ResolvedRequest {
        let request = Request::parse(text, Path::new("r.req.toml"), &|_| None).unwrap();
        let manifest = Manifest::parse("format = 1\nname = \"c\"\n", Path::new("m")).unwrap();
        interpolate(
            &request,
            &[],
            &manifest,
            &Scopes::new(Builtins::live(UNIX_EPOCH, 1)),
        )
        .unwrap()
        .0
    }

    fn header<'a>(wire: &'a Wire, name: &str) -> Option<&'a str> {
        wire.headers
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }

    #[test]
    fn query_is_appended_and_encoded() {
        let plain = resolve(
            "name = \"x\"\nurl = \"http://h/u\"\n[query]\nq = \"a b&c\"\nids = [\"1\", \"2\"]\n",
        );
        let more = resolve("name = \"x\"\nurl = \"http://h/u?x=1#top\"\n[query]\ny = \"2\"\n");

        assert_eq!(
            encode(&plain, Path::new("/c"), 1).unwrap().url,
            "http://h/u?q=a%20b%26c&ids=1&ids=2"
        );
        assert_eq!(
            encode(&more, Path::new("/c"), 1).unwrap().url,
            "http://h/u?x=1&y=2#top"
        );
    }

    #[test]
    fn json_and_text_get_a_default_content_type_unless_set() {
        let json = encode(
            &resolve("name = \"x\"\nurl = \"http://h\"\n[body]\njson = \"{}\"\n"),
            Path::new("/c"),
            1,
        )
        .unwrap();
        let text = encode(
            &resolve("name = \"x\"\nurl = \"http://h\"\n[body]\ntext = \"hi\"\n"),
            Path::new("/c"),
            1,
        )
        .unwrap();
        let own = encode(
            &resolve("name = \"x\"\nurl = \"http://h\"\n[headers]\ncontent-type = \"application/vnd.x+json\"\n[body]\njson = \"{}\"\n"),
            Path::new("/c"),
            1,
        )
        .unwrap();

        assert_eq!(
            (header(&json, "content-type"), json.body.as_ref()),
            (Some("application/json"), &b"{}"[..])
        );
        assert_eq!(
            header(&text, "content-type"),
            Some("text/plain; charset=utf-8")
        );
        assert_eq!(
            own.headers,
            vec![(
                "content-type".to_owned(),
                "application/vnd.x+json".to_owned()
            )],
            "kept, not doubled"
        );
    }

    #[test]
    fn form_is_urlencoded() {
        let w = encode(
            &resolve("name = \"x\"\nurl = \"http://h\"\n[body.form]\na = \"1 2\"\nb = \"x&y\"\n"),
            Path::new("/c"),
            1,
        )
        .unwrap();

        assert_eq!(w.body.as_ref(), b"a=1+2&b=x%26y");
        assert_eq!(
            header(&w, "content-type"),
            Some("application/x-www-form-urlencoded")
        );
    }

    #[test]
    fn multipart_text_and_file_parts() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.png"), b"PNG").unwrap();
        let r = resolve(
            "name = \"x\"\nurl = \"http://h\"\n[[body.multipart]]\nname = \"note\"\nvalue = \"hi\"\n[[body.multipart]]\nname = \"avatar\"\nfile = \"a.png\"\ncontent_type = \"image/png\"\n",
        );

        let w = encode(&r, dir.path(), 42).unwrap();
        let again = encode(&r, dir.path(), 42).unwrap();

        assert_eq!(w, again, "the seed fixes the boundary");
        insta::assert_snapshot!(format!(
            "{}\n\n{}",
            header(&w, "content-type").unwrap(),
            String::from_utf8_lossy(&w.body)
        ));
    }

    #[test]
    fn a_file_body_is_read_from_the_request_folder() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("up.bin"), [0u8, 1, 2]).unwrap();

        let w = encode(
            &resolve("name = \"x\"\nurl = \"http://h\"\n[body]\nfile = \"up.bin\"\n"),
            dir.path(),
            1,
        )
        .unwrap();

        assert_eq!(w.body.as_ref(), [0, 1, 2]);
        assert_eq!(header(&w, "content-type"), Some("application/octet-stream"));
    }

    #[test]
    fn a_missing_file_is_body_file() {
        let err = encode(
            &resolve("name = \"x\"\nurl = \"http://h\"\n[body]\nfile = \"gone.bin\"\n"),
            Path::new("/nonexistent"),
            1,
        )
        .unwrap_err();

        assert_eq!(err.code(), crate::codes::BODY_FILE, "code");
        assert!(err.to_string().contains("gone.bin"), "{err}");
    }
}
