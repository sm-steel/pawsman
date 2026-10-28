//! `keys.toml` overrides, end to end.
#![expect(
    clippy::unwrap_used,
    reason = "test helpers: rust rule 1 applies outside tests only"
)]

use std::time::Duration;

use pawsman_test_support::fake_http::{FakeHttp, Reply};

use crate::pty::{Session, Spec};

const F5: &[u8] = b"\x1b[15~";
const F6: &[u8] = b"\x1b[17~";

fn open(fake: &FakeHttp, keys: &str) -> (Session, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("pawsman.toml"),
        "format = 1\nname = \"T\"\n",
    )
    .unwrap();
    std::fs::write(
        dir.path().join("a.req.toml"),
        format!("name = \"A\"\nurl = \"{}\"\n", fake.url("/a")),
    )
    .unwrap();
    let s = Session::spawn(
        &Spec::new()
            .arg("--no-splash")
            .arg(dir.path().to_str().unwrap())
            .env("TERM", "xterm-256color")
            .replies(b"\x1b[?62c")
            .keys(keys),
    );
    s.wait_for_screen("GET    A");
    (s, dir)
}

#[test]
fn a_keys_toml_override_rebinds_send() {
    let fake = FakeHttp::start();
    fake.route("GET", "/a", Reply::new(200).body("ok"));
    let (s, _dir) = open(&fake, "[keys]\nsend = [\"F6\"]\n");

    s.write(F5);
    std::thread::sleep(Duration::from_millis(300));
    let after_f5 = fake.received().len();
    s.write(F6);
    s.wait_for_screen("200 OK");

    assert_eq!(after_f5, 0, "F5 no longer sends");
}

#[test]
fn an_invalid_keys_toml_shows_a_notice_and_keeps_defaults() {
    let fake = FakeHttp::start();
    fake.route("GET", "/a", Reply::new(200).body("ok"));
    let (s, _dir) = open(&fake, "[keys]\nlaunch = []\n");

    s.wait_for_screen("core::keys_invalid");
    s.write(F5);

    s.wait_for_screen("200 OK");
}
