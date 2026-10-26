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
