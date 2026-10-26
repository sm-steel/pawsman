//! Short messages shown in the status line.

use pawsman_error::ErrorInfo;

/// How a notice is styled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Level {
    /// Neutral information.
    Info,
    /// Something degraded.
    Warn,
    /// Something failed.
    Error,
}

/// A one-line message for the status line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notice {
    level: Level,
    text: String,
}

impl Notice {
    /// An information notice.
    pub fn info(text: impl Into<String>) -> Self {
        Self {
            level: Level::Info,
            text: text.into(),
        }
    }

    /// A warning notice.
    pub fn warn(text: impl Into<String>) -> Self {
        Self {
            level: Level::Warn,
            text: text.into(),
        }
    }

    /// An error notice: the code, then the message, so a truncated line still shows the code.
    pub fn from_error(err: &dyn ErrorInfo) -> Self {
        Self {
            level: Level::Error,
            text: format!("{}: {err}", err.code()),
        }
    }

    /// The level.
    #[must_use]
    pub fn level(&self) -> Level {
        self.level
    }

    /// The text.
    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notice_from_error_carries_the_code_first() {
        let err = crate::CoreError::no_home();

        let notice = Notice::from_error(&err);

        assert_eq!(notice.level(), Level::Error, "errors are Error notices");
        assert!(
            notice.text().starts_with("core::no_home: "),
            "code first: {}",
            notice.text()
        );
    }
}
