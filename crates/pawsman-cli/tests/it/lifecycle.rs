//! Start, quit, signals and the terminal left behind.

use std::time::Duration;

use crate::probe_paths::KITTY;
use crate::pty::{Session, Spec, contains};

#[test]
fn ctrl_q_restores_every_mode_newest_first() {
    let mut s = Session::spawn(
        &Spec::new()
            .arg("--no-splash")
            .env("TERM", "xterm-kitty")
            .replies(KITTY),
    );

    s.wait_for_screen("no collection open");
    s.write(b"\x11");
    s.wait_exit(Duration::from_secs(3));

    let tail =
        b"\x1b[?1006l\x1b[?1002l\x1b[?1000l\x1b[?1004l\x1b[?2004l\x1b[<u\x1b[?25h\x1b[?1049l";
    assert!(contains(&s.raw(), tail), "undo in order");
}

#[test]
fn the_splash_plays_then_home_shows() {
    let mut s = Session::spawn(
        &Spec::new()
            .env("TERM", "xterm-256color")
            .env("COLORTERM", "truecolor")
            .replies(b"\x1b[?62;22c"),
    );

    s.wait_for_screen("a terminal API client");
    s.wait_for_screen("no collection open");
    s.write(b"\x11");

    assert!(s.wait_exit(Duration::from_secs(3)).success(), "clean exit");
}

#[test]
fn sighup_restores_and_exits() {
    let mut s = Session::spawn(
        &Spec::new()
            .env("TERM", "xterm-256color")
            .replies(b"\x1b[?62;22c"),
    );

    s.wait_for_screen("a terminal API client");
    s.kill(rustix::process::Signal::HUP);
    let status = s.wait_exit(Duration::from_secs(1));

    assert!(
        status.code().is_some(),
        "exited by itself, not killed: {status:?}"
    );
    assert!(contains(&s.raw(), b"\x1b[?1049l"), "alternate screen left");
}

#[test]
fn redirected_stdout_stays_empty() {
    let out = tempfile::NamedTempFile::new().unwrap();
    let mut s = Session::spawn(
        &Spec::new()
            .arg("--no-splash")
            .env("TERM", "xterm-256color")
            .stdout(out.path())
            .replies(b"\x1b[?62;22c"),
    );

    s.wait_for_screen("no collection open");
    s.write(b"\x11");
    s.wait_exit(Duration::from_secs(3));

    assert_eq!(
        std::fs::metadata(out.path()).unwrap().len(),
        0,
        "frames went to /dev/tty"
    );
}

#[test]
fn invalid_settings_show_a_notice_and_defaults_apply() {
    let mut s = Session::spawn(
        &Spec::new()
            .arg("--no-splash")
            .env("TERM", "xterm-256color")
            .settings("[ui]\nanimation = false\n")
            .replies(b"\x1b[?62;22c"),
    );

    s.wait_for_screen("core::settings_invalid");
    s.write(b"\x11");

    assert!(s.wait_exit(Duration::from_secs(3)).success(), "still runs");
}

#[test]
#[expect(clippy::disallowed_methods, reason = "the test runs the binary")]
fn version_prints_without_a_terminal() {
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_pawsman"))
        .arg("--version")
        .output()
        .unwrap();

    assert!(
        String::from_utf8_lossy(&out.stdout).starts_with("pawsman "),
        "version line"
    );
}
