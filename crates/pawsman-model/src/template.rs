//! `{{…}}` templates: literal text and variable references.

use std::borrow::Cow;
use std::ops::Range;

/// A parsed template.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Template<'a>(Vec<Segment<'a>>);

/// One piece of a template.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Segment<'a> {
    /// Text sent as is.
    Literal(Cow<'a, str>),
    /// A variable; `span` covers `{{` through `}}`.
    Var {
        /// What the braces name.
        name: VarName<'a>,
        /// Byte range in the template text.
        span: Range<usize>,
    },
}

/// What a variable reference names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VarName<'a> {
    /// `{{name}}`: a scoped variable.
    Plain(&'a str),
    /// `{{env.NAME}}`: the process environment.
    Env(&'a str),
    /// `{{$name}}`: a built-in.
    Builtin(&'a str),
}

/// Why a template does not parse.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TemplateError {
    /// Where the problem starts.
    pub span: Range<usize>,
    /// What is wrong.
    pub reason: &'static str,
}

impl<'a> Template<'a> {
    /// The segments in order.
    #[must_use]
    pub fn segments(&self) -> &[Segment<'a>] {
        &self.0
    }
}

/// Parses `text`.
///
/// # Errors
///
/// An unterminated `{{`, empty braces, or a bad character in a name.
pub fn parse(text: &str) -> Result<Template<'_>, TemplateError> {
    let mut segments = Vec::new();
    let mut literal = String::new();
    let mut pos = 0;
    while let Some(found) = text.get(pos..).and_then(|rest| rest.find("{{")) {
        let open = pos + found;
        let before = text.get(pos..open).unwrap_or_default();
        if let Some(kept) = before.strip_suffix('\\') {
            literal.push_str(kept);
            literal.push_str("{{");
            pos = open + 2;
            continue;
        }
        literal.push_str(before);
        let close = text
            .get(open + 2..)
            .and_then(|rest| rest.find("}}"))
            .map(|n| open + 2 + n)
            .ok_or(TemplateError {
                span: open..text.len(),
                reason: "unterminated `{{`",
            })?;
        let span = open..close + 2;
        let name = name(text.get(open + 2..close).unwrap_or_default(), &span)?;
        if !literal.is_empty() {
            segments.push(Segment::Literal(Cow::Owned(std::mem::take(&mut literal))));
        }
        segments.push(Segment::Var { name, span });
        pos = close + 2;
    }
    literal.push_str(text.get(pos..).unwrap_or_default());
    if !literal.is_empty() {
        segments.push(Segment::Literal(Cow::Owned(literal)));
    }
    Ok(Template(segments))
}

fn name<'a>(inner: &'a str, span: &Range<usize>) -> Result<VarName<'a>, TemplateError> {
    let fail = |reason| TemplateError {
        span: span.clone(),
        reason,
    };
    let inner = inner.trim();
    if inner.is_empty() {
        return Err(fail("empty variable"));
    }
    let (name, bare) = if let Some(rest) = inner.strip_prefix('$') {
        (VarName::Builtin(rest), rest)
    } else if let Some(rest) = inner.strip_prefix("env.") {
        (VarName::Env(rest), rest)
    } else {
        (VarName::Plain(inner), inner)
    };
    let valid = |c: char| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-');
    if bare.is_empty() || !bare.chars().all(valid) {
        return Err(fail("bad character in a variable name"));
    }
    Ok(name)
}

/// Whether `text` references any variable; a broken template counts.
#[must_use]
pub fn has_vars(text: &str) -> bool {
    parse(text).map_or(true, |t| {
        t.0.iter().any(|s| matches!(s, Segment::Var { .. }))
    })
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;
    use rstest::rstest;

    use super::*;

    fn names(text: &str) -> Vec<String> {
        parse(text)
            .unwrap()
            .segments()
            .iter()
            .filter_map(|s| match s {
                Segment::Var { name, .. } => Some(format!("{name:?}")),
                Segment::Literal(_) => None,
            })
            .collect()
    }

    #[test]
    fn vars_env_and_builtins_are_told_apart() {
        assert_eq!(
            names("{{base}}/u/{{ id }}?t={{env.HOME}}&r={{$uuid}}"),
            [
                "Plain(\"base\")",
                "Plain(\"id\")",
                "Env(\"HOME\")",
                "Builtin(\"uuid\")"
            ],
        );
    }

    #[test]
    fn spans_cover_the_braces() {
        let t = parse("ab{{x}}cd").unwrap();

        assert!(
            matches!(t.segments().get(1), Some(Segment::Var { span, .. }) if *span == (2..7)),
            "{t:?}"
        );
    }

    #[test]
    fn an_escaped_open_is_literal() {
        let t = parse(r"a\{{b}}c").unwrap();

        assert_eq!(
            t.segments(),
            &[Segment::Literal("a{{b}}c".into())],
            "one literal"
        );
    }

    #[rstest]
    #[case::unterminated("a{{b", 1, "unterminated `{{`")]
    #[case::empty("{{ }}", 0, "empty variable")]
    #[case::bad_char("{{a b}}", 0, "bad character in a variable name")]
    #[case::dollar_inside("{{a$b}}", 0, "bad character in a variable name")]
    fn bad_templates_say_where(#[case] text: &str, #[case] start: usize, #[case] reason: &str) {
        let err = parse(text).unwrap_err();

        assert_eq!((err.span.start, err.reason), (start, reason), "{text:?}");
    }

    #[test]
    fn a_lone_close_is_literal() {
        assert!(names("a}}b").is_empty(), "no variable");
        assert!(!has_vars("a}}b"), "none");
        assert!(has_vars("a{{b}}"), "one");
    }

    proptest! {
        #[test]
        fn text_without_braces_is_one_literal(text in "[^{}\\\\]*") {
            let t = parse(&text).unwrap();
            let joined: String = t
                .segments()
                .iter()
                .map(|s| match s {
                    Segment::Literal(l) => l.to_string(),
                    Segment::Var { .. } => String::from("<var>"),
                })
                .collect();
            prop_assert_eq!(joined, text);
        }

        #[test]
        fn parse_never_panics(text in ".*") {
            let _ = parse(&text);
        }
    }
}
