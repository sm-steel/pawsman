//! Error codes of `pawsman-model`.

use pawsman_error::ErrorCode;

/// A directory has no `pawsman.toml`.
pub const NOT_A_COLLECTION: ErrorCode = ErrorCode::new("model::not_a_collection");
/// A collection or request file is not valid.
pub const INVALID_FILE: ErrorCode = ErrorCode::new("model::invalid_file");
/// A `{{…}}` template does not parse.
pub const BAD_TEMPLATE: ErrorCode = ErrorCode::new("model::bad_template");
/// A collection uses a newer format than this build reads.
pub const FORMAT_TOO_NEW: ErrorCode = ErrorCode::new("model::format_too_new");
/// A request names a sidecar body file that does not exist.
pub const MISSING_SIDECAR: ErrorCode = ErrorCode::new("model::missing_sidecar");
/// An environment declares a secret that has no value.
pub const MISSING_SECRET: ErrorCode = ErrorCode::new("model::missing_secret");

/// Every code, for the registry test.
pub const ALL: &[ErrorCode] = &[
    NOT_A_COLLECTION,
    INVALID_FILE,
    BAD_TEMPLATE,
    FORMAT_TOO_NEW,
    MISSING_SIDECAR,
];

#[cfg(test)]
mod tests {
    #[test]
    fn every_code_has_a_page() {
        pawsman_test_support::errors::assert_pages("model", super::ALL);
    }
}
