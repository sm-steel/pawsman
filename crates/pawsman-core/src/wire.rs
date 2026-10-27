//! Turning a resolved request into what goes on the wire: the URL with its query, the headers
//! with a default `Content-Type`, and the body bytes, reading body files from disk.

use std::path::Path;

use bytes::Bytes;
use pawsman_model::redact::{encode_form, encode_query};
use pawsman_model::{ResolvedBody, ResolvedPartValue, ResolvedRequest};

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
    let dir = root.join(resolved.base_dir());
    let read = |path: &Path| {
        let path = dir.join(path);
        std::fs::read(&path).map_err(|e| CoreError::body_file(&path, &e))
    };
    let mut headers = resolved.headers().to_vec();
    let (body, content_type) = match resolved.body() {
        None => (Vec::new(), None),
        Some(ResolvedBody::Json(text)) => (
            text.clone().into_bytes(),
            Some("application/json".to_owned()),
        ),
        Some(ResolvedBody::Text(text)) => (
            text.clone().into_bytes(),
            Some("text/plain; charset=utf-8".to_owned()),
        ),
        Some(ResolvedBody::Binary(bytes)) => (bytes.clone(), Some(OCTET_STREAM.to_owned())),
        Some(ResolvedBody::File(path)) => (read(path)?, Some(OCTET_STREAM.to_owned())),
        Some(ResolvedBody::Form(pairs)) => {
            let encoded: Vec<String> = pairs
                .iter()
                .map(|(k, v)| format!("{}={}", encode_form(k), encode_form(v)))
                .collect();
            (
                encoded.join("&").into_bytes(),
                Some("application/x-www-form-urlencoded".to_owned()),
            )
        }
        Some(ResolvedBody::Multipart(parts)) => {
            let boundary = boundary(seed);
            let mut out = Vec::new();
            for part in parts {
                out.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
                let name = quoted(part.name());
                match part.value() {
                    ResolvedPartValue::Text(text) => {
                        out.extend_from_slice(
                            format!("Content-Disposition: form-data; name=\"{name}\"\r\n")
                                .as_bytes(),
                        );
                        push_type(&mut out, part.content_type());
                        out.extend_from_slice(b"\r\n");
                        out.extend_from_slice(text.as_bytes());
                    }
                    ResolvedPartValue::File(path) => {
                        let file = path
                            .file_name()
                            .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
                        out.extend_from_slice(
                            format!(
                                "Content-Disposition: form-data; name=\"{name}\"; filename=\"{}\"\r\n",
                                quoted(&file)
                            )
                            .as_bytes(),
                        );
                        push_type(&mut out, Some(part.content_type().unwrap_or(OCTET_STREAM)));
                        out.extend_from_slice(b"\r\n");
                        out.extend_from_slice(&read(path)?);
                    }
                }
                out.extend_from_slice(b"\r\n");
            }
            out.extend_from_slice(format!("--{boundary}--\r\n").as_bytes());
            let own = format!("multipart/form-data; boundary={boundary}");
            let user_has_boundary =
                content_type_of(&headers).is_some_and(|v| v.contains("boundary="));
            if !user_has_boundary {
                headers.retain(|(n, _)| !n.eq_ignore_ascii_case(CONTENT_TYPE));
            }
            (out, Some(own))
        }
    };
    if let Some(content_type) = content_type
        && content_type_of(&headers).is_none()
    {
        headers.push((CONTENT_TYPE.to_owned(), content_type));
    }
    Ok(Wire {
        url: url_with_query(resolved.url(), resolved.query()),
        headers,
        body: Bytes::from(body),
    })
}

const CONTENT_TYPE: &str = "Content-Type";
const OCTET_STREAM: &str = "application/octet-stream";

fn content_type_of(headers: &[(String, String)]) -> Option<&str> {
    headers
        .iter()
        .find(|(n, _)| n.eq_ignore_ascii_case(CONTENT_TYPE))
        .map(|(_, v)| v.as_str())
}

fn push_type(out: &mut Vec<u8>, content_type: Option<&str>) {
    if let Some(content_type) = content_type {
        out.extend_from_slice(format!("Content-Type: {content_type}\r\n").as_bytes());
    }
}

/// A name or file name inside a quoted header parameter (RFC 7578 §4.2).
fn quoted(s: &str) -> String {
    s.replace('"', "%22")
        .replace('\r', "%0D")
        .replace('\n', "%0A")
}

fn boundary(seed: u64) -> String {
    let mut rng = fastrand::Rng::with_seed(seed);
    let tail: String = std::iter::repeat_with(|| rng.alphanumeric())
        .take(24)
        .collect();
    format!("pawsman-{tail}")
}

/// `url` with `query` appended, before any `#fragment`.
#[must_use]
pub fn url_with_query(url: &str, query: &[(String, String)]) -> String {
    if query.is_empty() {
        return url.to_owned();
    }
    let (base, fragment) = url
        .split_once('#')
        .map_or((url, None), |(b, f)| (b, Some(f)));
    let pairs: Vec<String> = query
        .iter()
        .map(|(k, v)| format!("{}={}", encode_query(k), encode_query(v)))
        .collect();
    let joiner = if base.contains('?') { '&' } else { '?' };
    let mut out = format!("{base}{joiner}{}", pairs.join("&"));
    if let Some(fragment) = fragment {
        out.push('#');
        out.push_str(fragment);
    }
    out
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

        let w = encode(&r, dir.path(), 7).unwrap();
        let again = encode(&r, dir.path(), 7).unwrap();

        assert_eq!(w, again, "the seed fixes the boundary");
        let body = String::from_utf8_lossy(&w.body).replace("\r\n", "\\r\\n\n");
        insta::assert_snapshot!(format!("{}\n\n{body}", header(&w, "content-type").unwrap()));
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
