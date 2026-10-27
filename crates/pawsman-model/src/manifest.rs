//! `pawsman.toml`: the collection's name, format and defaults.

use std::path::Path;

use crate::ModelError;

/// A parsed `pawsman.toml`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Manifest {
    format: u32,
    name: String,
    vars: Vec<(String, String)>,
    headers: Vec<(String, String)>,
}

impl Manifest {
    /// Parses the manifest; `path` names it in errors.
    ///
    /// # Errors
    ///
    /// `model::format_too_new` for a newer format; `model::invalid_file` for anything else wrong.
    pub fn parse(text: &str, path: &Path) -> Result<Self, ModelError> {
        let _ = text;
        Err(ModelError::invalid_file(path, "todo"))
    }

    /// The format the file was written in.
    #[must_use]
    pub fn format(&self) -> u32 {
        self.format
    }

    /// The display name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Collection variables, in file order.
    #[must_use]
    pub fn vars(&self) -> &[(String, String)] {
        &self.vars
    }

    /// Default headers, in file order.
    #[must_use]
    pub fn headers(&self) -> &[(String, String)] {
        &self.headers
    }
}

/// The string values of a TOML table in file order; `section` names it in errors.
pub(crate) fn strings(
    table: Option<toml::Table>,
    section: &str,
    path: &Path,
) -> Result<Vec<(String, String)>, ModelError> {
    let _ = (table, section);
    Err(ModelError::invalid_file(path, "todo"))
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use pawsman_error::ErrorInfo as _;
    use rstest::rstest;

    use super::*;
    use crate::codes;

    #[test]
    fn a_newer_format_is_too_new() {
        let err = Manifest::parse("format = 2\nname = \"x\"\n", Path::new("/c/pawsman.toml"))
            .unwrap_err();

        assert_eq!(err.code(), codes::FORMAT_TOO_NEW, "code");
        assert!(err.to_string().contains("format 2"), "{err}");
    }

    #[test]
    fn vars_and_headers_keep_file_order() {
        let m = Manifest::parse(
            "format = 1\nname = \"D\"\n[vars]\nz = \"1\"\na = \"2\"\n[headers]\nX-B = \"b\"\nX-A = \"a\"\n",
            Path::new("m"),
        )
        .unwrap();

        assert_eq!(
            m.vars(),
            &[("z".into(), "1".into()), ("a".into(), "2".into())],
            "vars order"
        );
        assert_eq!(
            m.headers().first().map(|h| h.0.as_str()),
            Some("X-B"),
            "headers order"
        );
        assert_eq!((m.name(), m.format()), ("D", 1), "name and format");
    }

    #[rstest]
    #[case::no_format("name = \"x\"\n")]
    #[case::zero("format = 0\nname = \"x\"\n")]
    #[case::string_format("format = \"1\"\nname = \"x\"\n")]
    #[case::unknown_key("format = 1\nname = \"x\"\nextra = 1\n")]
    #[case::number_var("format = 1\nname = \"x\"\n[vars]\nport = 8080\n")]
    fn bad_manifests_are_invalid_files(#[case] text: &str) {
        let err = Manifest::parse(text, Path::new("m")).unwrap_err();

        assert_eq!(err.code(), codes::INVALID_FILE, "{text:?}");
    }

    #[test]
    fn a_non_string_value_names_its_key() {
        let err = Manifest::parse(
            "format = 1\nname = \"x\"\n[vars]\nport = 8080\n",
            Path::new("m"),
        )
        .unwrap_err();

        assert!(
            err.to_string().contains("`vars.port` must be a string"),
            "{err}"
        );
    }
}
