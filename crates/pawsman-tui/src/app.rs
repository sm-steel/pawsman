//! The main loop and its state.

use std::collections::VecDeque;
use std::io::BufWriter;
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

use pawsman_core::keys::{self, Action, Context};
use pawsman_core::notice::Notice;
use ratatui::backend::{Backend, CrosstermBackend};
use ratatui::{Frame, Terminal};

use crate::input::{Input, Reader};
use crate::term::{Caps, InTmux, Term, probe};
use crate::views::splash::{self, Splash};
use crate::views::{help, home};
use crate::{Options, TuiError};

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
        let screen = match splash {
            SplashSetting::On => Screen::Splash(Splash::new(splash_seed(), caps.color())),
            SplashSetting::Off => Screen::Home,
        };
        Self {
            screen,
            overlay: Overlay::None,
            caps,
            notices: notices.into(),
            shown: None,
        }
    }

    fn context(&self) -> Context {
        match (&self.screen, self.overlay) {
            (Screen::Splash(_), _) => Context::Splash,
            (Screen::Home, Overlay::Help) => Context::Help,
            (Screen::Home, Overlay::None) => Context::Home,
        }
    }

    /// Applies one message.
    #[expect(
        clippy::needless_pass_by_value,
        reason = "messages hand over owned data (pasted text now, engine snapshots from 1.2)"
    )]
    pub(crate) fn update(&mut self, msg: Msg, _now: Duration) -> Result<Flow, TuiError> {
        let chord = match msg {
            Msg::Terminate => return Ok(Flow::Quit),
            Msg::InputClosed => return Err(TuiError::input_closed()),
            Msg::Input(Input::Key(chord)) => chord,
            Msg::Resize | Msg::Input(_) => return Ok(Flow::Continue),
        };
        let action = keys::lookup(self.context(), chord);
        if let Screen::Splash(splash) = &mut self.screen
            && action != Some(Action::Quit)
        {
            splash.skip();
            return Ok(Flow::Continue);
        }
        match action {
            Some(Action::Quit) => return Ok(Flow::Quit),
            Some(Action::Help) => self.overlay = Overlay::Help,
            Some(Action::Close) => self.overlay = Overlay::None,
            Some(Action::Copy) => self.notices.push_back(Notice::info(format!(
                "nothing selected · {} quits",
                quit_label()
            ))),
            None if !chord.mods().is_empty() => tracing::debug!(%chord, "unbound key"),
            Some(_) | None => {}
        }
        Ok(Flow::Continue)
    }

    /// Draws the current screen; a finished or too-small splash gives way to home first.
    pub(crate) fn draw(&mut self, frame: &mut Frame, now: Duration) {
        let area = frame.area();
        if let Screen::Splash(splash) = &mut self.screen {
            if !splash.done(now) && splash::fits(area) {
                splash.frame(now, frame.buffer_mut(), area);
                return;
            }
            self.screen = Screen::Home;
        }
        self.expire(now);
        home::draw(
            frame,
            area,
            self.shown.as_ref().map(|(notice, _)| notice),
            self.caps.hint(),
        );
        if self.overlay == Overlay::Help {
            help::draw(frame, area);
        }
    }

    fn expire(&mut self, now: Duration) {
        if self.shown.as_ref().is_some_and(|(_, until)| now >= *until) {
            self.shown = None;
        }
        if self.shown.is_none()
            && let Some(next) = self.notices.pop_front()
        {
            self.shown = Some((next, now + NOTICE_FOR));
        }
    }

    /// How long the loop may block before the next draw; `None` blocks until input.
    pub(crate) fn next_tick(&self, now: Duration) -> Option<Duration> {
        match &self.screen {
            Screen::Splash(splash) => splash.next_tick(now).or(Some(Duration::ZERO)),
            Screen::Home => match (&self.shown, self.notices.is_empty()) {
                (Some((_, until)), _) => Some(until.saturating_sub(now)),
                (None, false) => Some(Duration::ZERO),
                (None, true) => None,
            },
        }
    }
}

/// Runs the TUI until Ctrl-Q, a signal or an error; the terminal is restored on every path.
///
/// # Errors
///
/// Any `TuiError`: no terminal, a refused mode, a terminal write failure, or stopped input.
pub fn run(options: Options) -> Result<(), TuiError> {
    let mut term = Term::open()?;
    let probed = term.probe(InTmux::from_env(options.env()))?;
    let caps = Caps::resolve(
        &probe::parse(&probed.bytes),
        options.env(),
        options.images(),
    );
    tracing::info!(
        graphics = ?caps.graphics(),
        keyboard = ?caps.keyboard(),
        color = ?caps.color(),
        tmux = ?caps.tmux(),
        ssh = ?caps.ssh(),
        cell_px = ?caps.cell_px(),
        probe_ms = u64::try_from(probed.took.as_millis()).unwrap_or(u64::MAX),
        "probe"
    );
    term.enable_input(caps.keyboard());
    let (tx, rx) = std::sync::mpsc::channel();
    let reader = Reader::spawn(term.reader()?, tx).map_err(TuiError::terminal_io)?;
    let backend = CrosstermBackend::new(BufWriter::with_capacity(1 << 16, term.writer()?));
    let splash = options.splash();
    let result = Terminal::new(backend)
        .map_err(TuiError::terminal_io)
        .and_then(|mut terminal| {
            let mut app = App::new(splash, caps, options.into_notices());
            event_loop(&mut terminal, &mut app, &rx)
        });
    reader.stop();
    term.restore();
    result
}

/// Draws, then waits for the next message or tick; every queued message is handled before the
/// next draw, and the loop blocks without a timer when nothing on screen moves.
fn event_loop<B: Backend<Error = std::io::Error>>(
    terminal: &mut Terminal<B>,
    app: &mut App,
    rx: &Receiver<Msg>,
) -> Result<(), TuiError> {
    let start = Instant::now();
    let mut frames = 0u32;
    // No `terminal.clear()`: it asks the terminal for the cursor position and crossterm reads the
    // reply from the tty itself, racing the input thread. The alternate screen starts blank.
    loop {
        let now = start.elapsed();
        terminal
            .draw(|f| app.draw(f, now))
            .map_err(TuiError::terminal_io)?;
        frames += 1;
        let first = match app.next_tick(start.elapsed()) {
            Some(wait) => match rx.recv_timeout(wait) {
                Ok(msg) => Some(msg),
                Err(RecvTimeoutError::Timeout) => None,
                Err(RecvTimeoutError::Disconnected) => Some(Msg::InputClosed),
            },
            None => Some(rx.recv().unwrap_or(Msg::InputClosed)),
        };
        for msg in first
            .into_iter()
            .chain(std::iter::from_fn(|| rx.try_recv().ok()))
        {
            if app.update(msg, start.elapsed())? == Flow::Quit {
                tracing::info!(frames, "quit");
                return Ok(());
            }
        }
    }
}

/// The label of the first quit key, for hints.
fn quit_label() -> String {
    keys::table()
        .iter()
        .find(|b| b.action() == Action::Quit)
        .and_then(|b| b.chords().first())
        .map_or_else(|| "Ctrl-Q".to_owned(), ToString::to_string)
}

/// The splash's coalesce pattern changes from run to run.
fn splash_seed() -> u32 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(42, |d| d.subsec_nanos())
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
