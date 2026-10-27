//! Hiding secret values in text bound for logs, errors and the screen.

use std::borrow::Cow;

use secrecy::{ExposeSecret, SecretString};

/// The secret values one request used, and how to hide them.
#[derive(Debug, Clone, Default)]
pub struct Redactor {
    secrets: Vec<SecretString>,
}

impl Redactor {
    /// A redactor for `values`; empty values are ignored.
    #[must_use]
    pub fn new(values: Vec<SecretString>) -> Self {
        let mut secrets: Vec<SecretString> = values
            .into_iter()
            .filter(|v| !v.expose_secret().is_empty())
            .collect();
        secrets.sort_by_key(|v| std::cmp::Reverse(v.expose_secret().len()));
        Self { secrets }
    }

    /// Adds the secrets of `other`.
    pub fn extend(&mut self, other: &Self) {
        let mut all = std::mem::take(&mut self.secrets);
        all.extend(other.secrets.iter().cloned());
        *self = Self::new(all);
    }

    /// `text` with every secret, raw or percent-encoded, replaced by `***`.
    #[must_use]
    pub fn redact<'a>(&self, text: &'a str) -> Cow<'a, str> {
        self.replace(text, "***")
    }

    /// `text` with every secret replaced by `••••••`, for the screen.
    #[must_use]
    pub fn mask<'a>(&self, text: &'a str) -> Cow<'a, str> {
        self.replace(text, "••••••")
    }

    fn replace<'a>(&self, text: &'a str, with: &str) -> Cow<'a, str> {
        let mut out = Cow::Borrowed(text);
        for secret in &self.secrets {
            let raw = secret.expose_secret();
            for form in [raw.to_owned(), encode_query(raw), encode_form(raw)] {
                if out.contains(form.as_str()) {
                    out = Cow::Owned(out.replace(form.as_str(), with));
                }
            }
        }
        out
    }

    /// No secrets to hide.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.secrets.is_empty()
    }
}

/// Percent-encodes `s` for a URL query: everything but `A-Za-z0-9-._~` becomes `%XX`.
#[must_use]
pub fn encode_query(s: &str) -> String {
    encode(s, Space::Percent)
}

/// Encodes `s` for `application/x-www-form-urlencoded`: as [`encode_query`], with `+` for space.
#[must_use]
pub fn encode_form(s: &str) -> String {
    encode(s, Space::Plus)
}

/// How a space is encoded.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Space {
    Percent,
    Plus,
}

fn encode(s: &str, space: Space) -> String {
    use std::fmt::Write as _;

    let mut out = String::with_capacity(s.len());
    for byte in s.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(char::from(byte));
            }
            b' ' if space == Space::Plus => out.push('+'),
            _ => {
                let _ = write!(out, "%{byte:02X}");
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn redactor(values: &[&str]) -> Redactor {
        Redactor::new(values.iter().map(|v| SecretString::from(*v)).collect())
    }

    #[test]
    fn redactor_hides_raw_and_percent_encoded_forms() {
        let r = redactor(&["a b&c"]);

        assert_eq!(r.redact("?q=a%20b%26c&x=1"), "?q=***&x=1", "query form");
        assert_eq!(r.redact("q=a+b%26c"), "q=***", "form encoding");
        assert_eq!(r.redact("raw a b&c here"), "raw *** here", "raw");
        assert_eq!(r.mask("raw a b&c"), "raw ••••••", "mask");
        assert!(!r.is_empty(), "has secrets");
    }

    #[test]
    fn longest_first() {
        let r = redactor(&["ab", "abc"]);

        assert_eq!(r.redact("abc ab"), "*** ***");
    }

    #[test]
    fn empty_secrets_are_ignored() {
        let r = redactor(&[""]);

        assert!(r.is_empty(), "nothing to hide");
        assert_eq!(r.redact("text"), "text", "unchanged");
    }

    #[test]
    fn encoders_keep_unreserved_and_encode_utf8_per_byte() {
        assert_eq!(encode_query("aZ09-._~ /&=ж"), "aZ09-._~%20%2F%26%3D%D0%B6");
        assert_eq!(encode_form("a b+c"), "a+b%2Bc");
    }
}
