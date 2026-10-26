//! Each probe path, replayed with canned terminal replies.

use std::time::Duration;

use crate::pty::{Session, Spec, contains, probe_ms};

pub(crate) const KITTY: &[u8] =
    b"\x1b_Gi=31;OK\x1b\\\x1b[?1u\x1b[6;20;10t\x1bP1$r48:2:1:2:3m\x1b\\\x1b[?62;22c";
const TMUX_PASSTHROUGH: &[u8] = b"\x1b[?1;2;4c\x1b_Gi=31;OK\x1b\\\x1b[?62;22c";

fn quit(s: &mut Session) {
    s.write(b"\x11");
    s.wait_exit(Duration::from_secs(3));
}

#[test]
fn kitty_gets_kitty_graphics_and_keyboard_and_quits_on_kitty_ctrl_q() {
    let mut s = Session::spawn(
        &Spec::new()
            .arg("--no-splash")
            .env("TERM", "xterm-kitty")
            .replies(KITTY),
    );

    s.wait_for_screen("no collection open");
    s.write(b"\x1b[113;5u");
    let status = s.wait_exit(Duration::from_secs(3));

    assert!(status.success(), "clean exit: {status:?}");
    let log = s.log();
    assert!(
        log.contains("graphics=Kitty") && log.contains("keyboard=Kitty"),
        "probe result in the log:\n{log}"
    );
    assert!(
        contains(&s.raw(), b"\x1b[>1u"),
        "kitty keyboard flags pushed"
    );
}

#[test]
fn kitty_cyrillic_ctrl_q_quits() {
    let mut s = Session::spawn(
        &Spec::new()
            .arg("--no-splash")
            .env("TERM", "xterm-kitty")
            .replies(KITTY),
    );

    s.wait_for_screen("no collection open");
    s.write(b"\x1b[1081;5u");

    assert!(
        s.wait_exit(Duration::from_secs(3)).success(),
        "Ctrl-\u{439} quits"
    );
}

#[test]
fn tmux_with_passthrough_uses_the_outer_da1() {
    let mut s = Session::spawn(
        &Spec::new()
            .arg("--no-splash")
            .env("TERM", "tmux-256color")
            .env("TMUX", "/tmp/t,1,0")
            .replies(TMUX_PASSTHROUGH),
    );

    s.wait_for_screen("no collection open");
    quit(&mut s);

    let log = s.log();
    assert!(
        log.contains("graphics=Kitty") && log.contains("keyboard=Legacy"),
        "outer terminal decides:\n{log}"
    );
    assert!(
        contains(&s.raw(), b"\x1bPtmux;\x1b\x1b[c\x1b\\"),
        "second DA1 through passthrough"
    );
}

#[test]
fn tmux_without_passthrough_ends_the_probe_after_the_tmux_grace() {
    let mut s = Session::spawn(
        &Spec::new()
            .arg("--no-splash")
            .env("TERM", "tmux-256color")
            .env("TMUX", "/tmp/t,1,0")
            .replies(b"\x1b[?1;2;4c"),
    );

    s.wait_for_screen("allow-passthrough on");
    quit(&mut s);

    let log = s.log();
    assert!(
        log.contains("graphics=Blocks"),
        "tmux's sixel claim ignored:\n{log}"
    );
    let ms = probe_ms(&log);
    assert!((450..1000).contains(&ms), "probe took {ms} ms");
}

#[test]
fn tmux_without_utf8_turns_images_off_with_a_hint() {
    let mut s = Session::spawn(
        &Spec::new()
            .arg("--no-splash")
            .env("LANG", "C")
            .env("TERM", "tmux-256color")
            .env("TMUX", "/tmp/t,1,0")
            .replies(TMUX_PASSTHROUGH),
    );

    s.wait_for_screen("tmux with -u");
    quit(&mut s);

    assert!(s.log().contains("graphics=Off"), "off without UTF-8");
}

#[test]
fn xterm_gets_sixel_and_legacy_ctrl_q_quits() {
    let mut s = Session::spawn(
        &Spec::new()
            .arg("--no-splash")
            .env("TERM", "xterm")
            .replies(b"\x1b[?63;1;2;4;6;9;15;22c"),
    );

    s.wait_for_screen("no collection open");
    s.write(b"\x11");

    assert!(
        s.wait_exit(Duration::from_secs(3)).success(),
        "legacy Ctrl-Q quits"
    );
    assert!(s.log().contains("graphics=Sixel"), "sixel from DA1");
}

#[test]
fn silence_times_out_and_a_late_reply_types_nothing() {
    let mut s = Session::spawn(&Spec::new().arg("--no-splash").env("TERM", "xterm-256color"));

    s.wait_for_screen("no collection open");
    s.write(b"\x1b_Gi=31;OK\x1b\\\x1b[?62;22c");
    std::thread::sleep(Duration::from_millis(200));
    quit(&mut s);

    let log = s.log();
    let ms = probe_ms(&log);
    assert!(
        (1100..1600).contains(&ms),
        "probe waited 1 s + grace: {ms} ms"
    );
    assert!(
        log.contains("dropped a terminal reply"),
        "late reply dropped:\n{log}"
    );
    assert!(
        !log.contains("unbound key"),
        "nothing typed by the reply:\n{log}"
    );
}

#[test]
fn the_probe_runs_before_the_alternate_screen() {
    let s = Session::spawn(
        &Spec::new()
            .arg("--no-splash")
            .env("TERM", "tmux-256color")
            .env("TMUX", "/tmp/t,1,0")
            .replies(TMUX_PASSTHROUGH),
    );

    s.wait_for_screen("no collection open");

    let raw = s.raw();
    let at = |needle: &[u8]| raw.windows(needle.len()).position(|w| w == needle);
    let probe = at(b"\x1bPtmux;\x1b\x1b_G").expect("graphics query written");
    let alt = at(b"\x1b[?1049h").expect("alternate screen entered");
    assert!(
        probe < alt,
        "tmux drops passthrough replies after the alternate-screen switch"
    );
}

#[test]
fn outside_tmux_the_alternate_screen_covers_the_probe() {
    let s = Session::spawn(
        &Spec::new()
            .arg("--no-splash")
            .env("TERM", "linux")
            .replies(b"\x1b[?6c"),
    );

    s.wait_for_screen("no collection open");

    let raw = s.raw();
    let at = |needle: &[u8]| raw.windows(needle.len()).position(|w| w == needle);
    let probe = at(b"\x1b_Gi=31").expect("graphics query written");
    let alt = at(b"\x1b[?1049h").expect("alternate screen entered");
    assert!(
        alt < probe,
        "a terminal that prints the APC payload must print it on the alternate screen"
    );
}
