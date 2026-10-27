//! `.env`: secret values, one `KEY=value` per line.

use std::path::Path;

use secrecy::SecretString;

use crate::ModelError;

/// Parses a `.env` file. Bad lines are reported by number, never with their text.
#[must_use]
pub fn parse(text: &str, path: &Path) -> (Vec<(String, SecretString)>, Vec<ModelError>) {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut values = Vec::new();
    let mut issues = Vec::new();
    for (n, raw) in text.split('\n').enumerate() {
        let line = raw.strip_suffix('\r').unwrap_or(raw).trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        match pair(line) {
            Ok((key, value)) => values.push((key.to_owned(), SecretString::from(value))),
            Err(reason) => issues.push(ModelError::invalid_file(
                Path::new(&format!("{}:{}", path.display(), n + 1)),
                reason,
            )),
        }
    }
    (values, issues)
}

/// One `KEY=value` line; the error never holds the line's text.
fn pair(line: &str) -> Result<(&str, String), &'static str> {
    let line = line.strip_prefix("export ").map_or(line, str::trim_start);
    let (key, value) = line.split_once('=').ok_or("expected `KEY=value`")?;
    let key = key.trim();
    let valid = |c: char| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-');
    if key.is_empty() || !key.chars().all(valid) {
        return Err("expected `KEY=value`");
    }
    let value = value.trim();
    let value = if let Some(rest) = value.strip_prefix('\'') {
        let (inside, after) = rest.split_once('\'').ok_or("unterminated quote")?;
        trailing(after)?;
        inside.to_owned()
    } else if let Some(rest) = value.strip_prefix('"') {
        double_quoted(rest)?
    } else {
        value
            .split_once(" #")
            .map_or(value, |(v, _)| v)
            .trim_end()
            .to_owned()
    };
    Ok((key, value))
}

fn double_quoted(rest: &str) -> Result<String, &'static str> {
    let mut out = String::new();
    let mut chars = rest.char_indices();
    while let Some((i, c)) = chars.next() {
        match c {
            '"' => {
                trailing(rest.get(i + 1..).unwrap_or_default())?;
                return Ok(out);
            }
            '\\' => match chars.next().map(|(_, c)| c) {
                Some('n') => out.push('\n'),
                Some('t') => out.push('\t'),
                Some('r') => out.push('\r'),
                Some(other) => out.push(other),
                None => return Err("unterminated quote"),
            },
            other => out.push(other),
        }
    }
    Err("unterminated quote")
}

/// After a closing quote only a comment may follow.
fn trailing(after: &str) -> Result<(), &'static str> {
    let after = after.trim();
    if after.is_empty() || after.starts_with('#') {
        Ok(())
    } else {
        Err("text after the closing quote")
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use secrecy::ExposeSecret as _;

    use super::*;

    fn values(text: &str) -> Vec<(String, String)> {
        let (pairs, issues) = parse(text, Path::new(".env"));
        assert!(issues.is_empty(), "{issues:?}");
        pairs
            .into_iter()
            .map(|(k, v)| (k, v.expose_secret().to_owned()))
            .collect()
    }

    const DIALECTS: &str = "\u{feff}# comment\r\nA=1\r\nexport B = two\nC='lit $x \\n'\nD=\"a\\nb\"\nE=plain # trailing\nF=\"q \\\"x\\\" \\\\ \\t\"\nG=\n\n";

    #[test]
    fn dotenv_accepts_common_dialects() {
        let got = values(DIALECTS);

        let want: Vec<(String, String)> = [
            ("A", "1"),
            ("B", "two"),
            ("C", "lit $x \\n"),
            ("D", "a\nb"),
            ("E", "plain"),
            ("F", "q \"x\" \\ \t"),
            ("G", ""),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_owned(), v.to_owned()))
        .collect();
        assert_eq!(got, want);
    }

    #[test]
    fn bad_keys_and_text_after_quotes_are_issues() {
        for line in ["=value", "bad key=1", "A='x' junk", "B=\"x\" junk"] {
            let (pairs, issues) = parse(line, Path::new(".env"));
            assert!(
                pairs.is_empty() && issues.len() == 1,
                "{line:?}: {issues:?}"
            );
        }
        let (pairs, _) = parse("A='x' # comment", Path::new(".env"));
        assert_eq!(pairs.len(), 1, "a comment after a quote is fine");
    }

    #[test]
    fn a_bad_dotenv_line_names_the_line_not_the_value() {
        let (pairs, issues) = parse(
            "A=1\nthis-is-a-secret-without-equals\nB=\"opensesame\n",
            Path::new(".env"),
        );

        assert_eq!(pairs.len(), 1, "the good line still loads");
        let text: Vec<String> = issues.iter().map(ToString::to_string).collect();
        assert_eq!(text.len(), 2, "{text:?}");
        assert!(
            text.first().is_some_and(|t| t.contains(".env:2")),
            "{text:?}"
        );
        assert!(
            text.get(1).is_some_and(|t| t.contains(".env:3")),
            "{text:?}"
        );
        assert!(
            text.iter()
                .all(|t| !t.contains("this-is-a-secret") && !t.contains("opensesame")),
            "values never echoed: {text:?}"
        );
    }
}
