//! Error codes of `pawsman-tui`.

use pawsman_error::ErrorCode;

/// `/dev/tty` cannot be opened.
pub const NO_TTY: ErrorCode = ErrorCode::new("tui::no_tty");
/// A required terminal mode cannot be set.
pub const TERMINAL_MODE: ErrorCode = ErrorCode::new("tui::terminal_mode");
/// Writing to or reading from the terminal failed.
pub const TERMINAL_IO: ErrorCode = ErrorCode::new("tui::terminal_io");
/// The input thread stopped.
pub const INPUT_CLOSED: ErrorCode = ErrorCode::new("tui::input_closed");

/// Every code, for the registry test.
pub const ALL: &[ErrorCode] = &[NO_TTY, TERMINAL_MODE, TERMINAL_IO, INPUT_CLOSED];

#[cfg(test)]
mod tests {
    #[test]
    fn every_code_has_a_page() {
        pawsman_test_support::errors::assert_pages("tui", super::ALL);
    }
}
