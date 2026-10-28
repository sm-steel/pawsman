//! `pawsman init`.
#![expect(
    clippy::unwrap_used,
    reason = "test helpers: rust rule 1 applies outside tests only"
)]

use std::path::Path;
use std::process::{Command, Output};

fn init(dir: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_pawsman"))
        .env_clear()
        .env("HOME", dir)
        .arg("init")
        .arg(dir.join("api"))
        .output()
        .unwrap()
}

#[test]
fn init_creates_a_collection() {
    let dir = tempfile::tempdir().unwrap();

    let out = init(dir.path());

    assert!(out.status.success(), "{out:?}");
    assert!(
        String::from_utf8_lossy(&out.stdout).contains("created a collection in"),
        "{out:?}"
    );
    assert_eq!(
        std::fs::read_to_string(dir.path().join("api/pawsman.toml")).unwrap(),
        "format = 1\nname = \"api\"\n"
    );
    assert_eq!(
        std::fs::read_to_string(dir.path().join("api/.gitignore")).unwrap(),
        ".env\n.pawsman/\n"
    );
}

#[test]
fn init_twice_fails_with_exists() {
    let dir = tempfile::tempdir().unwrap();
    init(dir.path());

    let out = init(dir.path());

    assert!(!out.status.success(), "fails");
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("pawsman: core::exists:"),
        "{out:?}"
    );
}
