//! Writing a request back to its file, canonically: the same request always gives the same bytes.

use crate::Request;

/// A written request: the `*.req.toml` text and the sidecar body, if the body moved to one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Written {
    /// The request file's text.
    pub toml: String,
    /// The sidecar's file name and bytes.
    pub sidecar: Option<(String, Vec<u8>)>,
}

/// Writes `r`; `stem` is the file name without `.req.toml`, used to name a sidecar.
#[must_use]
pub fn request(r: &Request, stem: &str) -> Written {
    let _ = (r, stem);
    Written {
        toml: String::new(),
        sidecar: None,
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use proptest::prelude::*;

    use super::*;
    use crate::request::PartData;
    use crate::{Body, Content, Part, Script};

    const FULL: &str = r#"name = "Create user"
method = "POST"
url = "{{base}}/users"
docs = """
Creates a user.
Second line."""

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

[[assert]]
expr = "status == 200"

[script]
post = "vars.x = 1"
"#;

    fn no_sidecars(_: &str) -> Option<Vec<u8>> {
        None
    }

    fn request_with(body: Body) -> Request {
        let mut r = Request::parse(
            "name = \"n\"\nurl = \"http://h\"\n",
            Path::new("n"),
            &no_sidecars,
        )
        .unwrap();
        r.body = Some(body);
        r
    }

    fn reparse(w: &Written) -> Request {
        let lookup = |name: &str| {
            w.sidecar
                .as_ref()
                .filter(|(n, _)| n == name)
                .map(|(_, b)| b.clone())
        };
        Request::parse(&w.toml, Path::new("/c/r.req.toml"), &lookup).unwrap()
    }

    #[test]
    fn a_full_request_writes_canonically() {
        let r = Request::parse(FULL, Path::new("/c/create-user.req.toml"), &no_sidecars).unwrap();

        insta::assert_snapshot!(request(&r, "create-user").toml);
    }

    #[test]
    fn a_long_body_moves_to_a_sidecar() {
        let body = (0..41)
            .map(|n| format!("line {n}"))
            .collect::<Vec<_>>()
            .join("\n");
        let r = request_with(Body::Text(Content::Utf8(body.clone())));

        let w = request(&r, "notes");

        assert_eq!(
            w.sidecar.as_ref().map(|(n, b)| (n.as_str(), b.clone())),
            Some(("notes.body.txt", body.into_bytes())),
            "sidecar"
        );
        assert!(
            w.toml.contains("[body.text]\nsidecar = \"notes.body.txt\""),
            "{}",
            w.toml
        );
        assert_eq!(reparse(&w), r, "reads back");
    }

    #[test]
    fn forty_lines_stay_inline_and_binary_never_does() {
        let forty = (0..40)
            .map(|n| format!("l{n}"))
            .collect::<Vec<_>>()
            .join("\n");
        let inline = request(&request_with(Body::Json(Content::Utf8(forty))), "a");
        let binary = request(&request_with(Body::Json(Content::Binary(vec![0xff]))), "b");

        assert_eq!(inline.sidecar, None, "40 lines inline");
        assert_eq!(
            binary.sidecar,
            Some(("b.body.bin".into(), vec![0xff])),
            "binary to .bin"
        );
    }

    fn text() -> impl Strategy<Value = String> {
        "[ -~\u{80}-\u{10ffff}\n\t\r\"\\\\]{0,40}"
    }

    fn pairs() -> impl Strategy<Value = Vec<(String, String)>> {
        prop::collection::vec(("[A-Za-z][A-Za-z0-9_-]{0,12}", text()), 0..4).prop_map(|v| {
            let mut seen = std::collections::BTreeSet::new();
            v.into_iter()
                .filter(|(k, _)| seen.insert(k.clone()))
                .collect()
        })
    }

    fn table() -> impl Strategy<Value = toml::Table> {
        pairs().prop_map(|p| {
            p.into_iter()
                .map(|(k, v)| (k, toml::Value::String(v)))
                .collect()
        })
    }

    fn content() -> impl Strategy<Value = Content> {
        prop_oneof![
            prop::collection::vec(text(), 0..60).prop_map(|l| Content::Utf8(l.join("\n"))),
            any::<Vec<u8>>()
                .prop_filter("not utf-8", |b| std::str::from_utf8(b).is_err())
                .prop_map(Content::Binary),
        ]
    }

    fn part() -> impl Strategy<Value = Part> {
        (
            "[a-z]{1,8}",
            prop_oneof![
                text().prop_map(PartData::Text),
                "[a-z][a-z/._-]{0,15}".prop_map(|p| PartData::File(PathBuf::from(p))),
            ],
            prop::option::of("[a-z]+/[a-z]+"),
        )
            .prop_map(|(name, data, content_type)| Part {
                name,
                data,
                content_type,
            })
    }

    fn body() -> impl Strategy<Value = Option<Body>> {
        prop::option::of(prop_oneof![
            content().prop_map(Body::Json),
            content().prop_map(Body::Text),
            pairs().prop_map(Body::Form),
            prop::collection::vec(part(), 1..3).prop_map(Body::Multipart),
            "[a-z][a-z/._-]{0,15}".prop_map(|p| Body::File(PathBuf::from(p))),
        ])
    }

    fn query() -> impl Strategy<Value = Vec<(String, Vec<String>)>> {
        prop::collection::vec(
            ("[a-z][a-z0-9_]{0,8}", prop::collection::vec(text(), 1..4)),
            0..4,
        )
        .prop_map(|v| {
            let mut seen = std::collections::BTreeSet::new();
            v.into_iter()
                .filter(|(k, _)| seen.insert(k.clone()))
                .collect()
        })
    }

    prop_compose! {
        fn arb_request()(
            name in text(),
            method in prop::sample::select(vec!["GET", "POST", "PATCH", "X-CUSTOM"]),
            url in text(),
            docs in prop::option::of(text()),
            headers in pairs(),
            query in query(),
            vars in pairs(),
            body in body(),
            auth in prop::option::of(table()),
            captures in prop::collection::vec(table(), 0..3),
            asserts in prop::collection::vec(table(), 0..3),
            script in prop::option::of((prop::option::of(text()), prop::option::of(text()))),
        ) -> Request {
            Request {
                name,
                method: method.to_owned(),
                url,
                docs,
                headers,
                query,
                vars,
                body,
                auth,
                captures,
                asserts,
                script: script.map(|(pre, post)| Script { pre, post }),
            }
        }
    }

    proptest! {
        #[test]
        fn parse_write_parse_is_stable(r in arb_request()) {
            let w = request(&r, "r");
            let again = reparse(&w);

            prop_assert_eq!(&again, &r);
            prop_assert_eq!(request(&again, "r").toml, w.toml);
        }
    }
}
