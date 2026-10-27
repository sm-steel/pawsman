//! `.env`: secret values, one `KEY=value` per line.

use std::path::Path;

use secrecy::SecretString;

use crate::ModelError;

/// Parses a `.env` file. Bad lines are reported by number, never with their text.
#[must_use]
pub fn parse(text: &str, path: &Path) -> (Vec<(String, SecretString)>, Vec<ModelError>) {
    let _ = (text, path);
    (Vec::new(), Vec::new())
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
    fn a_bad_dotenv_line_names_the_line_not_the_value() {
        let (pairs, issues) = parse(
            "A=1\nthis-is-a-secret-without-equals\nB=\"unterminated\n",
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
                .all(|t| !t.contains("this-is-a-secret") && !t.contains("unterminated")),
            "values never echoed: {text:?}"
        );
    }
}
