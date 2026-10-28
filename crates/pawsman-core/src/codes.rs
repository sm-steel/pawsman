//! Error codes of `pawsman-core`.

use pawsman_error::ErrorCode;

/// Neither `HOME` nor `XDG_*` is set.
pub const NO_HOME: ErrorCode = ErrorCode::new("core::no_home");
/// `settings.toml` cannot be read or parsed.
pub const SETTINGS_INVALID: ErrorCode = ErrorCode::new("core::settings_invalid");
/// `globals.toml` cannot be read or parsed.
pub const GLOBALS_INVALID: ErrorCode = ErrorCode::new("core::globals_invalid");
/// A body file cannot be read.
pub const BODY_FILE: ErrorCode = ErrorCode::new("core::body_file");

/// A collection file cannot be written.
pub const WRITE: ErrorCode = ErrorCode::new("core::write");
/// A collection file cannot be read.
pub const READ: ErrorCode = ErrorCode::new("core::read");
/// The target of a create or rename already exists.
pub const EXISTS: ErrorCode = ErrorCode::new("core::exists");
/// The file watcher cannot start.
pub const WATCH: ErrorCode = ErrorCode::new("core::watch");
/// `keys.toml` cannot be read, parsed or applied.
pub const KEYS_INVALID: ErrorCode = ErrorCode::new("core::keys_invalid");

/// Every code, for the registry test.
pub const ALL: &[ErrorCode] = &[
    NO_HOME,
    SETTINGS_INVALID,
    GLOBALS_INVALID,
    BODY_FILE,
    WRITE,
    READ,
    EXISTS,
    WATCH,
    KEYS_INVALID,
];

#[cfg(test)]
mod tests {
    #[test]
    fn every_code_has_a_page() {
        pawsman_test_support::errors::assert_pages("core", super::ALL);
    }
}
