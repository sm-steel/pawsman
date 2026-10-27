//! Error codes of `pawsman-core`.

use pawsman_error::ErrorCode;

/// Neither `HOME` nor `XDG_*` is set.
pub const NO_HOME: ErrorCode = ErrorCode::new("core::no_home");
/// `settings.toml` cannot be read or parsed.
pub const SETTINGS_INVALID: ErrorCode = ErrorCode::new("core::settings_invalid");
/// `globals.toml` cannot be read or parsed.
pub const GLOBALS_INVALID: ErrorCode = ErrorCode::new("core::globals_invalid");

/// Every code, for the registry test.
pub const ALL: &[ErrorCode] = &[NO_HOME, SETTINGS_INVALID, GLOBALS_INVALID];

#[cfg(test)]
mod tests {
    #[test]
    fn every_code_has_a_page() {
        pawsman_test_support::errors::assert_pages("core", super::ALL);
    }
}
