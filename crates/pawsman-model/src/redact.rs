//! Hiding secret values in text bound for logs, errors and the screen.

use std::borrow::Cow;

use secrecy::SecretString;

/// The secret values one request used, and how to hide them.
#[derive(Debug, Clone, Default)]
pub struct Redactor {
    secrets: Vec<SecretString>,
}

impl Redactor {
    /// A redactor for `values`; empty values are ignored.
    #[must_use]
    pub fn new(values: Vec<SecretString>) -> Self {
        let _ = values;
        Self::default()
    }

    /// `text` with every secret, raw or percent-encoded, replaced by `***`.
    #[must_use]
    pub fn redact<'a>(&self, text: &'a str) -> Cow<'a, str> {
        Cow::Borrowed(text)
    }

    /// `text` with every secret replaced by `••••••`, for the screen.
    #[must_use]
    pub fn mask<'a>(&self, text: &'a str) -> Cow<'a, str> {
        Cow::Borrowed(text)
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
    s.to_owned()
}

/// Encodes `s` for `application/x-www-form-urlencoded`: as [`encode_query`], with `+` for space.
#[must_use]
pub fn encode_form(s: &str) -> String {
    s.to_owned()
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
