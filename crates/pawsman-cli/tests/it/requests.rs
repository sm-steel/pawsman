//! Opening a collection and sending requests, end to end against the fake HTTP server.
#![expect(
    clippy::unwrap_used,
    reason = "test helpers: rust rule 1 applies outside tests only"
)]

use std::time::{Duration, Instant};

use pawsman_test_support::fake_http::{FakeHttp, Reply};

use crate::pty::{Session, Spec};

fn collection(requests: &[(&str, String)]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("pawsman.toml"),
        "format = 1\nname = \"T\"\n",
    )
    .unwrap();
    for (file, text) in requests {
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

#[test]
fn f5_sends_and_the_response_appears() {
    let fake = FakeHttp::start();
    fake.route("GET", "/users", Reply::new(200).body("[\"ada\"]"));
    let dir = collection(&[(
        "users.req.toml",
        format!("name = \"Users\"\nurl = \"{}\"\n", fake.url("/users")),
    )]);
    let mut s = open(&dir);

    s.wait_for_screen("Users");
    s.write(b"\x1b[15~");
    s.wait_for_screen("200 OK");

    assert!(
        s.screen().contains("[\"ada\"]"),
        "body shown:\n{}",
        s.screen()
    );
    assert_eq!(fake.received().len(), 1, "one request");
    s.write(b"\x11");
    assert!(s.wait_exit(Duration::from_secs(3)).success(), "clean exit");
}

#[test]
fn esc_cancels_a_slow_request() {
    let fake = FakeHttp::start();
    fake.route(
        "GET",
        "/slow",
        Reply::new(200).delay(Duration::from_secs(10)),
    );
    let dir = collection(&[(
        "slow.req.toml",
        format!("name = \"Slow\"\nurl = \"{}\"\n", fake.url("/slow")),
    )]);
    let mut s = open(&dir);

    s.wait_for_screen("Slow");
    s.write(b"\x1b[15~");
    s.wait_for_screen("sending");
    s.write(b"\x1b");
    s.wait_for_screen("cancelled");

    s.write(b"\x11");
    assert!(s.wait_exit(Duration::from_secs(3)).success(), "clean exit");
}

#[test]
fn ctrl_q_during_a_request_exits() {
    let fake = FakeHttp::start();
    fake.route(
        "GET",
        "/slow",
        Reply::new(200).delay(Duration::from_secs(10)),
    );
    let dir = collection(&[(
        "slow.req.toml",
        format!("name = \"Slow\"\nurl = \"{}\"\n", fake.url("/slow")),
    )]);
    let mut s = open(&dir);
    s.wait_for_screen("Slow");
    s.write(b"\x1b[15~");
    s.wait_for_screen("sending");

    let start = Instant::now();
    s.write(b"\x11");
    let status = s.wait_exit(Duration::from_secs(3));

    assert!(status.success(), "clean exit: {status:?}");
    assert!(
        start.elapsed() < Duration::from_secs(1),
        "exited in {:?}",
        start.elapsed()
    );
}

#[test]
fn a_directory_that_is_not_a_collection_shows_the_error_on_home() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = open(&dir);

    s.wait_for_screen("model::not_a_collection");

    s.write(b"\x11");
    assert!(s.wait_exit(Duration::from_secs(3)).success(), "still runs");
}
