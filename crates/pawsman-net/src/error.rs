//! The net crate's error.

use std::borrow::Cow;
use std::fmt;

use pawsman_error::{ErrorCode, ErrorInfo, Retry};

use crate::codes;

/// Which deadline passed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Phase {
    /// Connecting (DNS, TCP and TLS).
    Connect,
    /// The whole request.
    Total,
}

/// An error from `pawsman-net`.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct NetError {
    kind: Kind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Kind {
    InvalidRequest { what: &'static str, value: String },
    Dns { host: String, reason: String },
    Connect { host: String, reason: String },
    Tls { host: String, reason: String },
    Timeout(Phase),
    Protocol(String),
    Spill(String),
}

impl NetError {
    /// `value` is not a valid `what` (method, url, header name, header value).
    #[must_use]
    pub fn invalid_request(what: &'static str, value: &str) -> Self {
        Self {
            kind: Kind::InvalidRequest {
                what,
                value: value.to_owned(),
            },
        }
    }

    /// `host` does not resolve.
    #[must_use]
    pub fn dns(host: &str, err: &dyn std::error::Error) -> Self {
        Self {
            kind: Kind::Dns {
                host: host.to_owned(),
                reason: err.to_string(),
            },
        }
    }

    /// Connecting to `host` failed.
    #[must_use]
    pub fn connect(host: &str, err: &dyn std::error::Error) -> Self {
        Self {
            kind: Kind::Connect {
                host: host.to_owned(),
                reason: err.to_string(),
            },
        }
    }

    /// The TLS handshake with `host` failed.
    #[must_use]
    pub fn tls(host: &str, reason: &str) -> Self {
        Self {
            kind: Kind::Tls {
                host: host.to_owned(),
                reason: reason.to_owned(),
            },
        }
    }

    /// A deadline passed.
    #[must_use]
    pub fn timeout(phase: Phase) -> Self {
        Self {
            kind: Kind::Timeout(phase),
        }
    }

    /// The server broke HTTP.
    #[must_use]
    pub fn protocol(reason: &str) -> Self {
        Self {
            kind: Kind::Protocol(reason.to_owned()),
        }
    }

    /// Writing the spilled body failed.
    #[must_use]
    pub fn spill(err: &dyn std::error::Error) -> Self {
        Self {
            kind: Kind::Spill(err.to_string()),
        }
    }
}

impl fmt::Display for NetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.kind {
            Kind::InvalidRequest { what, value } => write!(f, "invalid {what} `{value}`"),
            Kind::Dns { host, reason } => write!(f, "cannot resolve `{host}`: {reason}"),
            Kind::Connect { host, reason } => write!(f, "cannot connect to `{host}`: {reason}"),
            Kind::Tls { host, reason } => write!(f, "tls handshake with `{host}`: {reason}"),
            Kind::Timeout(Phase::Connect) => f.write_str("connection timed out"),
            Kind::Timeout(Phase::Total) => f.write_str("request timed out"),
            Kind::Protocol(reason) => write!(f, "bad response: {reason}"),
            Kind::Spill(reason) => write!(f, "cannot write the body to a temp file: {reason}"),
        }
    }
}

impl std::error::Error for NetError {}

impl ErrorInfo for NetError {
    fn code(&self) -> ErrorCode {
        match self.kind {
            Kind::InvalidRequest { .. } => codes::INVALID_REQUEST,
            Kind::Dns { .. } => codes::DNS,
            Kind::Connect { .. } => codes::CONNECT,
            Kind::Tls { .. } => codes::TLS,
            Kind::Timeout(_) => codes::TIMEOUT,
            Kind::Protocol(_) => codes::PROTOCOL,
            Kind::Spill(_) => codes::SPILL,
        }
    }

    fn help(&self) -> Option<Cow<'_, str>> {
        let help: Cow<'_, str> = match &self.kind {
            Kind::InvalidRequest { what, .. } => {
                Cow::Owned(format!("Fix the {what} in the request file."))
            }
            Kind::Dns { .. } => Cow::Borrowed("Check the host name and your network."),
            Kind::Connect { .. } => {
                Cow::Borrowed("Check that the server is running and the port is right.")
            }
            Kind::Tls { .. } => Cow::Borrowed("Check the URL scheme and the server's certificate."),
            Kind::Timeout(_) => {
                Cow::Borrowed("Try again, or raise `[http] timeout_ms` in the settings.")
            }
            Kind::Spill(_) => Cow::Borrowed("Free some disk space in the temp directory."),
            Kind::Protocol(_) => return None,
        };
        Some(help)
    }

    fn retry(&self) -> Retry {
        match self.kind {
            Kind::Dns { .. } | Kind::Connect { .. } | Kind::Timeout(_) | Kind::Spill(_) => {
                Retry::Retryable
            }
            Kind::InvalidRequest { .. } | Kind::Tls { .. } | Kind::Protocol(_) => Retry::Permanent,
        }
    }
}

#[cfg(test)]
mod tests {
    use pawsman_error::Retry;

    use super::*;

    fn io() -> std::io::Error {
        std::io::Error::other("boom")
    }

    #[test]
    fn retry_follows_the_spec() {
        assert_eq!(NetError::dns("h", &io()).retry(), Retry::Retryable, "dns");
        assert_eq!(
            NetError::connect("h", &io()).retry(),
            Retry::Retryable,
            "connect"
        );
        assert_eq!(
            NetError::timeout(Phase::Total).retry(),
            Retry::Retryable,
            "timeout"
        );
        assert_eq!(NetError::spill(&io()).retry(), Retry::Retryable, "spill");
        assert_eq!(
            NetError::tls("h", "bad cert").retry(),
            Retry::Permanent,
            "tls"
        );
        assert_eq!(
            NetError::invalid_request("url", "x").retry(),
            Retry::Permanent,
            "invalid"
        );
    }

    #[test]
    fn codes_and_messages() {
        let cases = [
            (
                NetError::invalid_request("url", "x y"),
                codes::INVALID_REQUEST,
                "invalid url `x y`",
            ),
            (
                NetError::dns("api.example", &io()),
                codes::DNS,
                "cannot resolve `api.example`: boom",
            ),
            (
                NetError::connect("h:80", &io()),
                codes::CONNECT,
                "cannot connect to `h:80`: boom",
            ),
            (
                NetError::tls("h", "bad cert"),
                codes::TLS,
                "tls handshake with `h`: bad cert",
            ),
            (
                NetError::timeout(Phase::Connect),
                codes::TIMEOUT,
                "connection timed out",
            ),
            (
                NetError::timeout(Phase::Total),
                codes::TIMEOUT,
                "request timed out",
            ),
            (
                NetError::protocol("early eof"),
                codes::PROTOCOL,
                "bad response: early eof",
            ),
            (
                NetError::spill(&io()),
                codes::SPILL,
                "cannot write the body to a temp file: boom",
            ),
        ];
        for (err, code, text) in cases {
            assert_eq!(err.code(), code, "{text}");
            assert_eq!(err.to_string(), text, "message");
        }
    }

    #[test]
    fn help_is_an_imperative_sentence_where_given() {
        for err in [
            NetError::invalid_request("url", "x"),
            NetError::dns("h", &io()),
            NetError::connect("h", &io()),
            NetError::tls("h", "r"),
            NetError::timeout(Phase::Total),
            NetError::spill(&io()),
        ] {
            let help = err.help().expect("help");
            assert!(
                help.ends_with('.') && help.starts_with(char::is_uppercase),
                "{help}"
            );
        }
        assert!(NetError::protocol("x").help().is_none(), "nothing to do");
    }
}
