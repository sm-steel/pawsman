//! Error codes of `pawsman-model`.

use pawsman_error::ErrorCode;

/// A directory has no `pawsman.toml`.
pub const NOT_A_COLLECTION: ErrorCode = ErrorCode::new("model::not_a_collection");
/// A collection or request file is not valid.
pub const INVALID_FILE: ErrorCode = ErrorCode::new("model::invalid_file");

/// Every code, for the registry test.
pub const ALL: &[ErrorCode] = &[NOT_A_COLLECTION, INVALID_FILE];

#[cfg(test)]
mod tests {
    #[test]
    fn every_code_has_a_page() {
        pawsman_test_support::errors::assert_pages("model", super::ALL);
    }
}
