//! The main loop and its state.

use crate::input::Input;

/// Everything the main loop reacts to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Msg {
    /// Decoded input.
    Input(Input),
    /// The terminal was resized.
    Resize,
    /// SIGTERM, SIGHUP or SIGINT.
    Terminate,
    /// The input thread hit end of file or an error.
    InputClosed,
}

use std::collections::VecDeque;
use std::time::Duration;

use pawsman_core::keys::{self, Action, Context};
use pawsman_core::notice::Notice;
use ratatui::Frame;

use crate::TuiError;
use crate::term::Caps;
use crate::views::splash::{self, Splash};
use crate::views::{help, home};

/// Whether the startup splash plays.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SplashSetting {
    /// Play it.
    On,
    /// Start on the main view.
    Off,
}

/// What the loop does after a message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Flow {
    Continue,
    Quit,
}

/// How long a notice stays on screen.
pub(crate) const NOTICE_FOR: Duration = Duration::from_secs(5);

enum Screen {
    Splash(Splash),
    Home,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Overlay {
    None,
    Help,
}

/// UI state: which screen, which overlay, which notice.
pub(crate) struct App {
    screen: Screen,
    overlay: Overlay,
    caps: Caps,
    notices: VecDeque<Notice>,
    /// The current notice and when it expires, set when it is first drawn.
    shown: Option<(Notice, Duration)>,
}

impl App {
    pub(crate) fn new(splash: SplashSetting, caps: Caps, notices: Vec<Notice>) -> Self {
        let _ = (
            splash,
            Splash::new,
            splash::fits,
            home::draw,
            help::draw,
            keys::lookup,
        );
        Self {
            screen: Screen::Home,
            overlay: Overlay::None,
            caps,
            notices: notices.into(),
            shown: None,
        }
    }

    pub(crate) fn update(&mut self, msg: Msg, now: Duration) -> Result<Flow, TuiError> {
        let _ = (
            msg,
            now,
            Action::Quit,
            Context::Home,
            TuiError::input_closed,
        );
        Ok(Flow::Continue)
    }

    pub(crate) fn draw(&mut self, frame: &mut Frame, now: Duration) {
        let _ = (
            frame,
            now,
            &self.screen,
            self.overlay,
            &self.caps,
            &self.notices,
            &self.shown,
        );
    }

    pub(crate) fn next_tick(&self, now: Duration) -> Option<Duration> {
        let _ = now;
        None
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use pawsman_core::env::EnvSnapshot;
    use pawsman_core::keys::{Chord, KeyCode};
    use pawsman_core::notice::Notice;
    use pawsman_core::settings::ImageMode;
    use pawsman_error::ErrorInfo as _;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    use super::*;
    use crate::input::Input;
    use crate::term::Caps;
    use crate::term::probe::parse;

    fn caps() -> Caps {
        let env = EnvSnapshot::from_pairs([
            ("COLORTERM", "truecolor"),
            ("TERM", "xterm-256color"),
            ("LANG", "C.UTF-8"),
        ]);
        Caps::resolve(&parse(b""), &env, ImageMode::Auto)
    }

    fn key(chord: Chord) -> Msg {
        Msg::Input(Input::Key(chord))
    }

    fn render(app: &mut App, w: u16, h: u16, now: Duration) -> Terminal<TestBackend> {
        let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
        terminal.draw(|f| app.draw(f, now)).unwrap();
        terminal
    }

    fn screen(terminal: &Terminal<TestBackend>) -> String {
        terminal.backend().to_string()
    }

    #[test]
    fn ctrl_q_quits_from_the_splash() {
        let mut app = App::new(SplashSetting::On, caps(), vec![]);

        let flow = app
            .update(key(Chord::ctrl('q')), Duration::from_millis(100))
            .unwrap();

        assert_eq!(flow, Flow::Quit, "quit during the splash");
    }

    #[test]
    fn ctrl_q_quits_from_home() {
        let mut app = App::new(SplashSetting::Off, caps(), vec![]);

        let flow = app.update(key(Chord::ctrl('q')), Duration::ZERO).unwrap();

        assert_eq!(flow, Flow::Quit, "quit on home");
    }

    #[test]
    fn the_splash_shows_first() {
        let mut app = App::new(SplashSetting::On, caps(), vec![]);

        let text = screen(&render(&mut app, 80, 24, Duration::from_millis(900)));

        assert!(text.contains("a terminal API client"), "splash:\n{text}");
        assert_eq!(
            app.next_tick(Duration::from_millis(900)),
            Some(Duration::from_millis(16)),
            "animating"
        );
    }

    #[test]
    fn any_other_key_skips_to_home() {
        let mut app = App::new(SplashSetting::On, caps(), vec![]);

        app.update(
            key(Chord::plain(KeyCode::Char('x'))),
            Duration::from_millis(100),
        )
        .unwrap();
        let terminal = render(&mut app, 80, 24, Duration::from_millis(116));

        assert!(
            screen(&terminal).contains("no collection open"),
            "home after a key"
        );
        assert_eq!(
            app.next_tick(Duration::from_millis(116)),
            None,
            "no timer on a still home"
        );
    }

    #[test]
    fn splash_ends_by_itself() {
        let mut app = App::new(SplashSetting::On, caps(), vec![]);
        let mut now = Duration::ZERO;
        let mut frames = 0;
        while let Some(tick) = app.next_tick(now) {
            render(&mut app, 80, 24, now);
            now += tick;
            frames += 1;
            assert!(now < Duration::from_secs(3), "splash never ended");
        }

        let terminal = render(&mut app, 80, 24, now);

        assert!(frames > 10, "it animated: {frames} frames");
        assert!(
            screen(&terminal).contains("no collection open"),
            "home after the splash"
        );
    }

    #[test]
    fn splash_off_starts_on_home() {
        let mut app = App::new(SplashSetting::Off, caps(), vec![]);

        let terminal = render(&mut app, 80, 24, Duration::ZERO);

        assert!(
            screen(&terminal).contains("no collection open"),
            "home at once"
        );
    }

    #[test]
    fn f1_opens_help_and_esc_closes_it() {
        let mut app = App::new(SplashSetting::Off, caps(), vec![]);

        app.update(key(Chord::plain(KeyCode::F(1))), Duration::ZERO)
            .unwrap();
        let open = screen(&render(&mut app, 80, 24, Duration::ZERO));
        app.update(key(Chord::plain(KeyCode::Esc)), Duration::ZERO)
            .unwrap();
        let closed = screen(&render(&mut app, 80, 24, Duration::ZERO));

        assert!(
            open.contains("Ctrl-Q") && open.contains("copy the selection"),
            "help lists the table:\n{open}"
        );
        assert!(!closed.contains("copy the selection"), "closed:\n{closed}");
    }

    #[test]
    fn ctrl_c_with_nothing_selected_names_ctrl_q() {
        let mut app = App::new(SplashSetting::Off, caps(), vec![]);

        app.update(key(Chord::ctrl('c')), Duration::ZERO).unwrap();
        let text = screen(&render(&mut app, 80, 24, Duration::ZERO));

        assert!(
            text.contains("nothing selected") && text.contains("Ctrl-Q quits"),
            "hint:\n{text}"
        );
        assert_eq!(
            app.next_tick(Duration::ZERO),
            Some(NOTICE_FOR),
            "a timer for the notice to expire"
        );
    }

    #[test]
    fn notices_expire() {
        let mut app = App::new(
            SplashSetting::Off,
            caps(),
            vec![Notice::warn("no log file")],
        );
        let first = screen(&render(&mut app, 80, 24, Duration::ZERO));

        let later = screen(&render(
            &mut app,
            80,
            24,
            NOTICE_FOR + Duration::from_millis(1),
        ));

        assert!(first.contains("no log file"), "shown:\n{first}");
        assert!(!later.contains("no log file"), "gone after {NOTICE_FOR:?}");
    }

    #[test]
    fn terminate_quits_and_input_closed_is_an_error() {
        let mut app = App::new(SplashSetting::Off, caps(), vec![]);

        let flow = app.update(Msg::Terminate, Duration::ZERO).unwrap();
        let err = app.update(Msg::InputClosed, Duration::ZERO).unwrap_err();

        assert_eq!(flow, Flow::Quit, "signal quits");
        assert_eq!(err.code(), crate::codes::INPUT_CLOSED, "closed input");
    }

    #[test]
    fn tiny_areas_never_panic() {
        for (w, h) in [(0, 0), (1, 1), (10, 3), (44, 5), (48, 7)] {
            let mut app = App::new(SplashSetting::On, caps(), vec![Notice::info("x")]);
            render(&mut app, w, h, Duration::from_millis(300));
            app.update(key(Chord::plain(KeyCode::F(1))), Duration::from_millis(300))
                .unwrap();
            app.update(key(Chord::plain(KeyCode::F(1))), Duration::from_millis(300))
                .unwrap();
            render(&mut app, w, h, Duration::from_millis(400));
        }
    }

    #[test]
    fn a_too_small_terminal_ends_the_splash() {
        let mut app = App::new(SplashSetting::On, caps(), vec![]);

        render(&mut app, 20, 5, Duration::from_millis(100));

        assert_eq!(
            app.next_tick(Duration::from_millis(100)),
            None,
            "splash over below the logo size"
        );
    }

    #[test]
    fn home_snapshot_80x24() {
        let mut app = App::new(SplashSetting::Off, caps(), vec![]);

        let terminal = render(&mut app, 80, 24, Duration::ZERO);

        insta::assert_snapshot!(terminal.backend());
    }

    #[test]
    fn help_snapshot_80x24() {
        let mut app = App::new(SplashSetting::Off, caps(), vec![]);
        app.update(key(Chord::plain(KeyCode::F(1))), Duration::ZERO)
            .unwrap();

        let terminal = render(&mut app, 80, 24, Duration::ZERO);

        insta::assert_snapshot!(terminal.backend());
    }

    #[test]
    fn caps_hint_shows_on_home() {
        let env = EnvSnapshot::from_pairs([("TMUX", "/t,1,0"), ("LANG", "C")]);
        let caps = Caps::resolve(&parse(b""), &env, ImageMode::Auto);
        let mut app = App::new(SplashSetting::Off, caps, vec![]);

        let text = screen(&render(&mut app, 80, 24, Duration::ZERO));

        assert!(text.contains("tmux with -u"), "UTF-8 hint:\n{text}");
    }

    #[test]
    fn mouse_paste_and_focus_change_nothing() {
        let mut app = App::new(SplashSetting::Off, caps(), vec![]);
        let before = screen(&render(&mut app, 80, 24, Duration::ZERO));

        for msg in [
            Msg::Input(Input::Paste("x".into())),
            Msg::Input(Input::FocusLost),
            Msg::Resize,
        ] {
            assert_eq!(
                app.update(msg, Duration::ZERO).unwrap(),
                Flow::Continue,
                "continues"
            );
        }
        let after = screen(&render(&mut app, 80, 24, Duration::ZERO));

        assert_eq!(before, after, "same screen");
    }
}
