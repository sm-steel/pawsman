//! The TUI crate's error.

use std::borrow::Cow;
use std::fmt;
use std::io;

use pawsman_error::{ErrorCode, ErrorInfo};

use crate::codes;
use crate::term::modes::Mode;

/// An error from `pawsman-tui`.
#[derive(Debug)]
#[non_exhaustive]
pub struct TuiError {
    kind: Kind,
}

#[derive(Debug)]
enum Kind {
    NoTty(io::Error),
    TerminalMode {
        mode: &'static str,
        source: io::Error,
    },
    TerminalIo(io::Error),
    InputClosed,
}

impl TuiError {
    pub(crate) fn no_tty(source: io::Error) -> Self {
        Self {
            kind: Kind::NoTty(source),
        }
    }

    pub(crate) fn terminal_mode(mode: Mode, source: io::Error) -> Self {
        Self {
            kind: Kind::TerminalMode {
                mode: mode.name(),
                source,
            },
        }
    }

    pub(crate) fn terminal_io(source: io::Error) -> Self {
        Self {
            kind: Kind::TerminalIo(source),
        }
    }

    pub(crate) fn input_closed() -> Self {
        Self {
            kind: Kind::InputClosed,
        }
    }
}

impl fmt::Display for TuiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.kind {
            Kind::NoTty(err) => write!(f, "cannot open the terminal device: {err}"),
            Kind::TerminalMode { mode, source } => {
                write!(f, "terminal refused {mode} mode: {source}")
            }
            Kind::TerminalIo(err) => write!(f, "terminal write: {err}"),
            Kind::InputClosed => f.write_str("input reader stopped"),
        }
    }
}

impl std::error::Error for TuiError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match &self.kind {
            Kind::NoTty(err) | Kind::TerminalIo(err) | Kind::TerminalMode { source: err, .. } => {
                Some(err)
            }
            Kind::InputClosed => None,
        }
    }
}

impl ErrorInfo for TuiError {
    fn code(&self) -> ErrorCode {
        match self.kind {
            Kind::NoTty(_) => codes::NO_TTY,
            Kind::TerminalMode { .. } => codes::TERMINAL_MODE,
            Kind::TerminalIo(_) => codes::TERMINAL_IO,
            Kind::InputClosed => codes::INPUT_CLOSED,
        }
    }

    fn help(&self) -> Option<Cow<'_, str>> {
        match self.kind {
            Kind::NoTty(_) => Some(Cow::Borrowed(
                "Run pawsman in a terminal; over SSH use `ssh -t`.",
            )),
            Kind::TerminalMode { .. } => {
                Some(Cow::Borrowed("Run pawsman in a full terminal emulator."))
            }
            Kind::TerminalIo(_) | Kind::InputClosed => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::error::Error as _;

    use super::*;

    fn io() -> io::Error {
        io::Error::other("boom")
    }

    #[test]
    fn messages_and_sources() {
        let cases = [
            (
                TuiError::no_tty(io()),
                "cannot open the terminal device: boom",
            ),
            (
                TuiError::terminal_mode(Mode::AltScreen, io()),
                "terminal refused alternate screen mode: boom",
            ),
            (TuiError::terminal_io(io()), "terminal write: boom"),
            (TuiError::input_closed(), "input reader stopped"),
        ];

        for (err, text) in &cases {
            assert_eq!(err.to_string(), *text, "message");
        }
        let with_source = cases.iter().filter(|(e, _)| e.source().is_some()).count();
        assert_eq!(with_source, 3, "the I/O errors carry their cause");
    }

    #[test]
    fn help_where_there_is_something_to_do() {
        let tty = TuiError::no_tty(io());
        let mode = TuiError::terminal_mode(Mode::Raw, io());

        assert_eq!(
            tty.help().as_deref(),
            Some("Run pawsman in a terminal; over SSH use `ssh -t`."),
            "no tty"
        );
        assert!(
            mode.help().is_some_and(|h| h.ends_with('.')),
            "refused mode"
        );
        assert!(TuiError::input_closed().help().is_none(), "nothing to do");
    }
}
