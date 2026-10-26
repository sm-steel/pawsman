//! The error-code registry check: every code is unique, in its crate's namespace, and has a page.

use std::collections::BTreeSet;
use std::path::PathBuf;

use pawsman_error::ErrorCode;

/// Panics naming every code in `codes` that is duplicated, outside `namespace`, or without a page
/// at `docs/errors/<namespace>/<slug>.md`.
///
/// # Panics
///
/// When any code fails a check; the message lists every problem.
pub fn assert_pages(namespace: &str, codes: &[ErrorCode]) {
    let docs = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../docs/errors");
    let mut seen = BTreeSet::new();
    let mut problems = Vec::new();
    for code in codes {
        if !seen.insert(code.as_str()) {
            problems.push(format!("{code}: listed twice"));
        }
        if code.namespace() != namespace {
            problems.push(format!("{code}: not in `{namespace}`"));
        }
        let page = docs
            .join(code.namespace())
            .join(format!("{}.md", code.slug()));
        if !page.is_file() {
            problems.push(format!("{code}: no page at {}", page.display()));
        }
    }
    assert!(
        problems.is_empty(),
        "error registry: {}",
        problems.join("; ")
    );
}
