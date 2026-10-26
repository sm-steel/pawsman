//! Visual scenarios: real views on `TestBackend`, written to `target/visual/tui-*.png`.
//! Run with `cargo test -p pawsman-tui visual -- --ignored` and read every file.

mod png;

use std::path::PathBuf;
use std::time::Duration;

use pawsman_core::env::EnvSnapshot;
use pawsman_core::keys::{Chord, KeyCode};
use pawsman_core::notice::Notice;
use pawsman_core::settings::ImageMode;
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

use crate::app::{App, Msg, SplashSetting};
use crate::input::Input;
use crate::term::probe::parse;
use crate::term::{Caps, ColorDepth};
use crate::views::splash::Splash;

fn out(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/visual")
        .join(format!("tui-{name}.png"))
}

fn caps(pairs: &[(&str, &str)]) -> Caps {
    Caps::resolve(
        &parse(b""),
        &EnvSnapshot::from_pairs(pairs.iter().copied()),
        ImageMode::Auto,
    )
}

fn truecolor() -> Caps {
    caps(&[
        ("COLORTERM", "truecolor"),
        ("TERM", "xterm-256color"),
        ("LANG", "C.UTF-8"),
    ])
}

fn splash_frame(ms: u64, depth: ColorDepth) -> Buffer {
    let area = Rect::new(0, 0, 80, 24);
    let mut buf = Buffer::empty(area);
    let mut splash = Splash::new(42, depth);
    splash.frame(Duration::from_millis(ms), &mut buf, area);
    buf
}

fn app_frame(app: &mut App, w: u16, h: u16) -> Buffer {
    let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
    terminal.draw(|f| app.draw(f, Duration::ZERO)).unwrap();
    terminal.backend().buffer().clone()
}

#[test]
#[ignore = "writes PNG files for the visual check"]
fn visual_splash_frames() {
    let mut r = png::Renderer::new();
    for ms in [0, 300, 600, 900, 1200] {
        r.render(
            &splash_frame(ms, ColorDepth::TrueColor),
            &out(&format!("splash-{ms:04}ms")),
        );
    }
    r.render(
        &splash_frame(600, ColorDepth::Indexed256),
        &out("splash-0600ms-256"),
    );
    r.render(&splash_frame(0, ColorDepth::Ansi16), &out("splash-16"));
}

#[test]
#[ignore = "writes PNG files for the visual check"]
fn visual_home_and_help() {
    let mut r = png::Renderer::new();
    let mut home = App::new(SplashSetting::Off, truecolor(), vec![]);
    r.render(&app_frame(&mut home, 80, 24), &out("home-80x24"));
    r.render(&app_frame(&mut home, 160, 48), &out("home-160x48"));
    r.render(&app_frame(&mut home, 30, 8), &out("home-30x8"));
    home.update(
        Msg::Input(Input::Key(Chord::plain(KeyCode::F(1)))),
        Duration::ZERO,
    )
    .unwrap();
    r.render(&app_frame(&mut home, 80, 24), &out("help-80x24"));

    let err = Notice::from_error(&crate::TuiError::input_closed());
    let mut notice = App::new(SplashSetting::Off, truecolor(), vec![err]);
    r.render(&app_frame(&mut notice, 80, 24), &out("home-notice"));

    let tmux = caps(&[("TMUX", "/t,1,0"), ("LANG", "C"), ("TERM", "screen")]);
    let mut hint = App::new(SplashSetting::Off, tmux, vec![]);
    r.render(&app_frame(&mut hint, 80, 24), &out("home-tmux-hint"));
}
