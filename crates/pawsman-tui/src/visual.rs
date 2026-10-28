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

fn demo_state() -> pawsman_core::state::AppState {
    use std::path::{Path, PathBuf};

    let files = [
        ("1-users.req.toml", "name = \"List users\"\nurl = \"http://127.0.0.1:8787/users\"\n\n[headers]\nAccept = \"application/json\"\n"),
        ("2-echo.req.toml", "name = \"Create user\"\nmethod = \"POST\"\nurl = \"http://127.0.0.1:8787/echo\"\n\n[body]\ntext = \"{\\\"name\\\": \\\"Ada\\\"}\"\n"),
        ("3-slow.req.toml", "name = \"Slow\"\nurl = \"http://127.0.0.1:8787/slow\"\n"),
        ("4-delete.req.toml", "name = \"Delete user\"\nmethod = \"DELETE\"\nurl = \"http://127.0.0.1:8787/users/1\"\n"),
        ("5-broken.req.toml", "name = "),
    ]
    .iter()
    .map(|(n, t)| pawsman_model::SourceFile::new(PathBuf::from(n), t.as_bytes().to_vec()))
        .chain(std::iter::once(pawsman_model::SourceFile::new(
            "pawsman.toml".into(),
            b"format = 1\nname = \"Demo\"\n".to_vec(),
        )))
    .collect();
    pawsman_core::state::AppState::new(
        pawsman_model::Collection::from_files(Path::new("/demo"), files),
        pawsman_core::resolve::Context::default(),
    )
}

fn main_app(state: &mut pawsman_core::state::AppState, downs: usize) -> App {
    let mut app = App::new(SplashSetting::Off, truecolor(), vec![]);
    app.update(Msg::Snapshot(state.snapshot()), Duration::ZERO)
        .unwrap();
    for _ in 0..downs {
        app.update(
            Msg::Input(Input::Key(Chord::plain(KeyCode::Down))),
            Duration::ZERO,
        )
        .unwrap();
    }
    app
}

#[test]
#[ignore = "writes PNG files for the visual check"]
fn visual_main_view() {
    use std::time::Instant;

    use pawsman_core::state::Event;
    use pawsman_model::RequestId;
    use pawsman_net::{NetError, NetEvent, Response};

    let mut r = png::Renderer::new();
    let mut st = demo_state();
    let id = RequestId::new(1);
    st.apply(
        Event::Send {
            id,
            at: std::time::UNIX_EPOCH,
            seed: 1,
        },
        Instant::now(),
    );
    let body = br#"[{"id":1,"name":"Ada Lovelace"},{"id":2,"name":"Grace Hopper"}]"#;
    let response = Response::for_tests(200, body, Duration::from_millis(42));
    st.apply(Event::Net(NetEvent::Done { id, response }), Instant::now());
    st.apply(
        Event::Send {
            id: RequestId::new(3),
            at: std::time::UNIX_EPOCH,
            seed: 1,
        },
        Instant::now(),
    );
    let failed = RequestId::new(4);
    st.apply(
        Event::Send {
            id: failed,
            at: std::time::UNIX_EPOCH,
            seed: 1,
        },
        Instant::now(),
    );
    let error = NetError::connect(
        "127.0.0.1:8787",
        &std::io::Error::other("connection refused"),
    );
    st.apply(
        Event::Net(NetEvent::Failed { id: failed, error }),
        Instant::now(),
    );

    let mut done = main_app(&mut st, 0);
    r.render(&app_frame(&mut done, 160, 48), &out("main-160x48-done"));
    r.render(&app_frame(&mut done, 100, 30), &out("main-100x30-done"));
    let mut sending = main_app(&mut st, 2);
    r.render(
        &app_frame(&mut sending, 160, 48),
        &out("main-160x48-sending"),
    );
    let mut fail = main_app(&mut st, 3);
    r.render(&app_frame(&mut fail, 100, 30), &out("main-100x30-failed"));
    let mut broken = main_app(&mut st, 4);
    r.render(
        &app_frame(&mut broken, 160, 48),
        &out("main-160x48-invalid"),
    );
    let mut narrow = main_app(&mut st, 0);
    narrow
        .update(
            Msg::Input(Input::Key(Chord::plain(KeyCode::BackTab))),
            Duration::ZERO,
        )
        .unwrap();
    r.render(&app_frame(&mut narrow, 60, 20), &out("main-60x20-response"));
}

fn press(app: &mut App, keys: &[Chord]) {
    for key in keys {
        app.update(Msg::Input(Input::Key(*key)), Duration::ZERO)
            .unwrap();
    }
}

/// Selects the best palette match for `query`.
fn goto(app: &mut App, query: &str) {
    press(app, &[Chord::ctrl('k')]);
    let typed: Vec<Chord> = query
        .chars()
        .map(|c| Chord::plain(KeyCode::Char(c)))
        .collect();
    press(app, &typed);
    press(app, &[Chord::plain(KeyCode::Enter)]);
}

fn demo_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/demo")
}

#[test]
#[ignore = "writes PNG files for the visual check"]
fn visual_environments() {
    use std::path::Path;
    use std::time::Instant;

    use pawsman_core::env::EnvSnapshot;
    use pawsman_core::resolve::Context;
    use pawsman_core::state::{AppState, Event};
    use pawsman_model::RequestId;

    let mut r = png::Renderer::new();
    let env = EnvSnapshot::from_pairs([("PAWSMAN_SECRET_TOKEN", "s3cr3t")]);
    let mut st = AppState::new(
        pawsman_core::store::load(&demo_dir()),
        Context::new(vec![], env),
    );
    let e = Chord::plain(KeyCode::Char('e'));
    let t = Chord::plain(KeyCode::Char('t'));
    let down = Chord::plain(KeyCode::Down);

    let mut tree = main_app(&mut st, 0);
    r.render(
        &app_frame(&mut tree, 160, 48),
        &out("envs-160x48-tree-none"),
    );
    press(&mut tree, &[e]);
    r.render(&app_frame(&mut tree, 160, 48), &out("envs-160x48-picker"));

    st.apply(Event::SelectEnv(Some("dev".into())), Instant::now());
    let mut traced = main_app(&mut st, 0);
    press(&mut traced, &[t]);
    r.render(
        &app_frame(&mut traced, 160, 48),
        &out("envs-160x48-trace-secret"),
    );
    r.render(
        &app_frame(&mut traced, 80, 24),
        &out("envs-80x24-trace-secret"),
    );

    let template = st
        .snapshot()
        .collection()
        .and_then(|c| c.entries().iter().find(|e| e.stem() == "template"))
        .map(|e| e.id())
        .unwrap();
    st.apply(
        Event::Send {
            id: template,
            at: std::time::UNIX_EPOCH,
            seed: 1,
        },
        Instant::now(),
    );
    let mut unresolved = main_app(&mut st, 0);
    goto(&mut unresolved, "unresolved");
    r.render(
        &app_frame(&mut unresolved, 160, 48),
        &out("envs-160x48-unresolved"),
    );

    let mut sidecar = main_app(&mut st, 0);
    goto(&mut sidecar, "create");
    press(&mut sidecar, &[t]);
    r.render(
        &app_frame(&mut sidecar, 100, 30),
        &out("envs-100x30-sidecar-body"),
    );

    let mut narrow = main_app(&mut st, 0);
    press(&mut narrow, &[down]);
    r.render(&app_frame(&mut narrow, 60, 20), &out("envs-60x20-sidebar"));
}

#[test]
#[ignore = "writes PNG files for the visual check"]
fn visual_tree_and_palette() {
    use pawsman_core::resolve::Context;
    use pawsman_core::state::AppState;

    let mut r = png::Renderer::new();
    let mut st = AppState::new(pawsman_core::store::load(&demo_dir()), Context::default());
    let key = |c: char| Chord::plain(KeyCode::Char(c));
    let up = Chord::plain(KeyCode::Up);
    let left = Chord::plain(KeyCode::Left);

    let mut tree = main_app(&mut st, 0);
    goto(&mut tree, "users");
    press(&mut tree, &[up, left]);
    r.render(
        &app_frame(&mut tree, 160, 48),
        &out("tree-160x48-collapsed-folder"),
    );

    let mut prompt = main_app(&mut st, 0);
    goto(&mut prompt, "list users");
    press(&mut prompt, &[key('r')]);
    r.render(
        &app_frame(&mut prompt, 80, 24),
        &out("tree-80x24-rename-prompt"),
    );
    press(&mut prompt, &[Chord::plain(KeyCode::Esc), key('a')]);
    let typed: Vec<Chord> = "Ещё один запрос — 日本".chars().map(key).collect();
    press(&mut prompt, &typed);
    r.render(
        &app_frame(&mut prompt, 80, 24),
        &out("tree-80x24-new-wide-chars"),
    );

    let mut ask = main_app(&mut st, 0);
    goto(&mut ask, "slow");
    press(&mut ask, &[up, key('d')]);
    r.render(
        &app_frame(&mut ask, 80, 24),
        &out("tree-80x24-delete-folder"),
    );

    for (w, h) in [(80, 24), (160, 48)] {
        let mut palette = main_app(&mut st, 0);
        press(&mut palette, &[Chord::ctrl('k')]);
        press(&mut palette, &[key('u'), key('s')]);
        r.render(
            &app_frame(&mut palette, w, h),
            &out(&format!("palette-{w}x{h}")),
        );
    }

    let depth256 = caps(&[("TERM", "xterm-256color"), ("LANG", "C.UTF-8")]);
    let mut indexed = App::new(SplashSetting::Off, depth256, vec![]);
    indexed
        .update(Msg::Snapshot(st.snapshot()), Duration::ZERO)
        .unwrap();
    press(&mut indexed, &[Chord::ctrl('k'), key('m'), key('e')]);
    r.render(
        &app_frame(&mut indexed, 100, 30),
        &out("palette-100x30-256"),
    );

    let mut help = main_app(&mut st, 0);
    press(&mut help, &[Chord::plain(KeyCode::F(1))]);
    r.render(&app_frame(&mut help, 80, 24), &out("help-80x24-main"));
    r.render(&app_frame(&mut help, 160, 48), &out("help-160x48-main"));
}
