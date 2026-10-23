//! Error metadata shared by every pawsman crate.
//!
//! Each crate keeps its own typed error and implements [`ErrorInfo`] on it, so the edge (TUI notices,
//! `pawsman run` reports) can render a message, a help line and decide whether to retry.
#![forbid(unsafe_code)]

use std::borrow::Cow;
use std::fmt;

/// A namespaced, stable error code such as `net::tls_handshake`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ErrorCode(&'static str);

impl ErrorCode {
    /// Creates a code; use one constant per failure in the owning crate's `codes` module.
    #[must_use]
    pub const fn new(code: &'static str) -> Self {
        Self(code)
    }

    /// The code as text.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        self.0
    }

    /// The part before `::`, which names the docs folder (`docs/errors/<namespace>/`).
    #[must_use]
    pub fn namespace(self) -> &'static str {
        self.0
            .split_once("::")
            .map_or("", |(namespace, _)| namespace)
    }

    /// The part after `::`, which names the docs page (`docs/errors/<namespace>/<slug>.md`).
    #[must_use]
    pub fn slug(self) -> &'static str {
        self.0.split_once("::").map_or(self.0, |(_, slug)| slug)
    }
}

impl fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}

/// Whether repeating the same operation later may succeed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Retry {
    /// Transient: network, rate limit, server restart.
    Retryable,
    /// Permanent until the input changes.
    Permanent,
}

/// Metadata every error in the workspace exposes to the edge.
pub trait ErrorInfo: std::error::Error {
    /// Stable machine-readable code.
    fn code(&self) -> ErrorCode;

    /// One imperative sentence telling the user what to do, if there is something to do. It may
    /// name the bad input, so it can borrow from the error.
    fn help(&self) -> Option<Cow<'_, str>> {
        None
    }

    /// Whether the operation may succeed if repeated.
    fn retry(&self) -> Retry {
        Retry::Permanent
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn code_displays_as_text() {
        let code = ErrorCode::new("net::tls_handshake");

        let shown = code.to_string();

        assert_eq!(shown, "net::tls_handshake", "Display is the raw code");
    }

    #[test]
    fn code_splits_into_namespace_and_slug() {
        let code = ErrorCode::new("model::unknown_var");

        let parts = (code.namespace(), code.slug());

        assert_eq!(parts, ("model", "unknown_var"), "split at the first `::`");
    }

    #[test]
    fn code_without_separator_is_all_slug() {
        let code = ErrorCode::new("internal");

        let parts = (code.namespace(), code.slug());

        assert_eq!(
            parts,
            ("", "internal"),
            "no namespace when there is no `::`"
        );
    }

    #[derive(Debug)]
    struct Fake;

    impl fmt::Display for Fake {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("fake")
        }
    }

    impl std::error::Error for Fake {}

    impl ErrorInfo for Fake {
        fn code(&self) -> ErrorCode {
            ErrorCode::new("test::fake")
        }
    }

    #[derive(Debug)]
    struct UnknownVar(String);

    impl fmt::Display for UnknownVar {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "unknown variable `{}`", self.0)
        }
    }

    impl std::error::Error for UnknownVar {}

    impl ErrorInfo for UnknownVar {
        fn code(&self) -> ErrorCode {
            ErrorCode::new("model::unknown_var")
        }

        fn help(&self) -> Option<Cow<'_, str>> {
            Some(
                format!(
                    "Define `{}` in the active environment or in `.env`.",
                    self.0
                )
                .into(),
            )
        }
    }

    #[test]
    fn help_can_name_the_bad_input() {
        let err = UnknownVar("token".to_owned());

        let help = err.help();

        assert_eq!(
            help.as_deref(),
            Some("Define `token` in the active environment or in `.env`."),
            "the docs' example help line is expressible"
        );
    }

    #[test]
    fn defaults_are_no_help_and_permanent() {
        let err = Fake;

        let meta = (err.help(), err.retry());

        assert_eq!(meta, (None, Retry::Permanent), "trait defaults");
    }
}
