//! The tree against the files on disk: edits from outside, broken files, and changes made in the
//! TUI that touch only their own lines.
#![expect(
    clippy::unwrap_used,
    reason = "test helpers: rust rule 1 applies outside tests only"
)]

use std::path::Path;
use std::time::{Duration, Instant};

use crate::pty::{Session, Spec};

const ENTER: &[u8] = b"\r";
const BACKSPACE: &[u8] = b"\x7f";

fn collection(files: &[(&str, &str)]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("pawsman.toml"),
        "format = 1\nname = \"T\"\n",
    )
    .unwrap();
    for (file, text) in files {
        std::fs::write(dir.path().join(file), text).unwrap();
    }
    dir
}

fn open(dir: &tempfile::TempDir) -> Session {
    Session::spawn(
        &Spec::new()
            .arg("--no-splash")
            .arg(dir.path().to_str().unwrap())
            .env("TERM", "xterm-256color")
            .replies(b"\x1b[?62c"),
    )
}

/// Waits up to 5 s for `done`.
fn until(what: &str, done: impl Fn() -> bool) {
    let start = Instant::now();
    while !done() {
        assert!(
            start.elapsed() < Duration::from_secs(5),
            "timed out: {what}"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_default()
}

/// `after` equals `before` except that the lines starting with `key` differ.
fn only_changed(before: &str, after: &str, key: &str) {
    let strip = |text: &str| -> Vec<String> {
        text.lines()
            .map(|l| {
                if l.starts_with(key) {
                    key.to_owned()
                } else {
                    l.to_owned()
                }
            })
            .collect()
    };
    assert_eq!(
        strip(before),
        strip(after),
        "only `{key}` changes:\n{after}"
    );
    assert_ne!(before, after, "`{key}` changed");
}

#[test]
fn an_external_edit_shows_within_a_second() {
    let dir = collection(&[("a.req.toml", "name = \"Alpha\"\nurl = \"http://h/a\"\n")]);
    let s = open(&dir);
    s.wait_for_screen("Alpha");
    std::thread::sleep(Duration::from_millis(100));

    let written = Instant::now();
    std::fs::write(
        dir.path().join("a.req.toml"),
        "name = \"Bravo\"\nurl = \"http://h/a\"\n",
    )
    .unwrap();
    s.wait_for_screen("Bravo");

    assert!(
        written.elapsed() < Duration::from_millis(1500),
        "took {:?}",
        written.elapsed()
    );
}

#[test]
fn broken_toml_becomes_an_error_node() {
    let dir = collection(&[("a.req.toml", "name = \"Alpha\"\nurl = \"http://h/a\"\n")]);
    let s = open(&dir);
    s.wait_for_screen("Alpha");
    std::thread::sleep(Duration::from_millis(100));

    std::fs::write(dir.path().join("a.req.toml"), "name = ").unwrap();

    s.wait_for_screen("a.req.toml");
    s.wait_for_screen("model::invalid_file");
}

const FOLDER: &str = "# order by hand\norder = [\"a\", \"b\"]\n";
const A: &str =
    "# the first one\nname = \"A\"\nurl = \"http://h/a\"\n\n[headers]\nAccept = \"x\"\n";
const B: &str = "name = \"B\"\nurl = \"http://h/b\"\n";

#[test]
fn rename_and_reorder_change_only_their_lines() {
    let dir = collection(&[
        ("_folder.toml", FOLDER),
        ("a.req.toml", A),
        ("b.req.toml", B),
    ]);
    let s = open(&dir);
    s.wait_for_screen("GET    A");

    s.write(b"r");
    s.wait_for_screen("Rename A");
    s.write(BACKSPACE);
    s.write(b"Alpha");
    s.write(ENTER);
    let renamed = dir.path().join("alpha.req.toml");
    until("the renamed file", || renamed.exists());
    s.wait_for_screen("GET    Alpha");
    s.write(b"J");
    let folder = dir.path().join("_folder.toml");
    until("the new order", || {
        read(&folder).contains("[\"b\", \"alpha\"]")
    });

    assert!(!dir.path().join("a.req.toml").exists(), "old file gone");
    only_changed(A, &read(&renamed), "name = ");
    only_changed(FOLDER, &read(&folder), "order = ");
    assert_eq!(read(&dir.path().join("b.req.toml")), B, "b untouched");
}

#[test]
fn delete_asks_first() {
    let dir = collection(&[("a.req.toml", A), ("b.req.toml", B)]);
    let s = open(&dir);
    s.wait_for_screen("GET    A");

    s.write(b"d");
    s.wait_for_screen("Delete request A?");
    s.write(b"n");
    std::thread::sleep(Duration::from_millis(200));
    assert!(dir.path().join("a.req.toml").exists(), "n keeps it");
    s.write(b"d");
    s.wait_for_screen("Delete request A?");
    s.write(b"y");

    until("the file removed", || {
        !dir.path().join("a.req.toml").exists()
    });
}
