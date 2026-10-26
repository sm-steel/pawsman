//! The main loop and its state.

use std::collections::VecDeque;
use std::io::BufWriter;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

use pawsman_core::engine::Engine;
use pawsman_core::keys::{self, Action, Context};
use pawsman_core::notice::Notice;
use pawsman_core::state::Snapshot;
use pawsman_model::RequestId;
use ratatui::backend::{Backend, CrosstermBackend};
use ratatui::{Frame, Terminal};

use crate::input::MouseKind;
use crate::input::{Input, Reader};
use crate::term::{Caps, InTmux, Term, probe};
use crate::views::main::{self, Focus, MainView};
use crate::views::splash::{self, Splash};
use crate::views::{colors, help, home};
use crate::{Options, TuiError};

/// Everything the main loop reacts to.
#[derive(Debug, Clone)]
pub(crate) enum Msg {
    /// Decoded input.
    Input(Input),
    /// The terminal was resized.
    Resize,
    /// SIGTERM, SIGHUP or SIGINT.
    Terminate,
    /// The input thread hit end of file or an error.
    InputClosed,
    /// The engine published a new state.
    Snapshot(Arc<Snapshot>),
}

impl PartialEq for Msg {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Input(a), Self::Input(b)) => a == b,
            (Self::Snapshot(a), Self::Snapshot(b)) => Arc::ptr_eq(a, b),
            (Self::Resize, Self::Resize)
            | (Self::Terminate, Self::Terminate)
            | (Self::InputClosed, Self::InputClosed) => true,
            _ => false,
        }
    }
}

/// What the UI asks the engine to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum UiCommand {
    /// Send this request.
    Send(RequestId),
    /// Cancel it.
    Cancel(RequestId),
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
    Main,
}

/// How often the screen redraws while a request is in flight.
const SENDING_TICK: Duration = Duration::from_millis(100);
/// Lines a page key scrolls.
const PAGE: u16 = 10;

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
    snapshot: Option<Arc<Snapshot>>,
    selected: Option<RequestId>,
    focus: Focus,
    scroll: u16,
    commands: Vec<UiCommand>,
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
            snapshot: None,
            selected: None,
            focus: Focus::Sidebar,
            scroll: 0,
            commands: Vec::new(),
        }
    }

    /// The screen after the splash: the collection if one is open, else home.
    fn landing(&self) -> Screen {
        if self
            .snapshot
            .as_ref()
            .is_some_and(|s| s.collection().is_some())
        {
            Screen::Main
        } else {
            Screen::Home
        }
    }

    fn take_snapshot(&mut self, snapshot: Arc<Snapshot>) {
        self.notices.extend(snapshot.notices().iter().cloned());
        let fresh_error = snapshot.load_error().is_some()
            && self
                .snapshot
                .as_ref()
                .is_none_or(|s| s.load_error().is_none());
        if fresh_error && let Some(err) = snapshot.load_error() {
            self.notices.push_back(Notice::from_error(err));
        }
        if self.selected.is_none() {
            self.selected = snapshot
                .collection()
                .and_then(|c| c.entries().first())
                .map(pawsman_model::Entry::id);
        }
        self.snapshot = Some(snapshot);
        if matches!(self.screen, Screen::Home) {
            self.screen = self.landing();
        }
    }

    /// Moves the selection by `step` entries, clamped to the list.
    fn select(&mut self, step: isize) {
        let Some(entries) = self
            .snapshot
            .as_ref()
            .and_then(|s| s.collection())
            .map(pawsman_model::Collection::entries)
        else {
            return;
        };
        let current = entries
            .iter()
            .position(|e| Some(e.id()) == self.selected)
            .unwrap_or(0);
        let next = current
            .saturating_add_signed(step)
            .min(entries.len().saturating_sub(1));
        self.selected = entries.get(next).map(pawsman_model::Entry::id);
        self.scroll = 0;
    }

    fn main_action(&mut self, action: Action) {
        match action {
            Action::Send => {
                if let Some(id) = self.selected {
                    self.commands.push(UiCommand::Send(id));
                }
            }
            Action::Cancel => {
                let sending = self
                    .snapshot
                    .as_ref()
                    .zip(self.selected)
                    .is_some_and(|(s, id)| {
                        matches!(
                            s.status(id),
                            pawsman_core::state::RequestStatus::Sending { .. }
                        )
                    });
                if sending && let Some(id) = self.selected {
                    self.commands.push(UiCommand::Cancel(id));
                }
            }
            Action::NextPane => self.focus = self.focus.next(),
            Action::PrevPane => self.focus = self.focus.prev(),
            Action::Up | Action::Down => {
                let step = if action == Action::Up { -1 } else { 1 };
                match self.focus {
                    Focus::Sidebar => self.select(step),
                    Focus::Response => self.scroll_by(if step < 0 { -1 } else { 1 }),
                    Focus::Request => {}
                }
            }
            Action::PageUp => self.scroll_by(-i32::from(PAGE)),
            Action::PageDown => self.scroll_by(i32::from(PAGE)),
            _ => {}
        }
    }

    fn scroll_by(&mut self, step: i32) {
        self.scroll = u16::try_from((i32::from(self.scroll) + step).max(0)).unwrap_or(u16::MAX);
    }

    fn context(&self) -> Context {
        match (&self.screen, self.overlay) {
            (Screen::Splash(_), _) => Context::Splash,
            (Screen::Home | Screen::Main, Overlay::Help) => Context::Help,
            (Screen::Home, Overlay::None) => Context::Home,
            (Screen::Main, Overlay::None) => Context::Main,
        }
    }

    /// Applies one message.
    pub(crate) fn update(&mut self, msg: Msg, _now: Duration) -> Result<Flow, TuiError> {
        let chord = match msg {
            Msg::Terminate => return Ok(Flow::Quit),
            Msg::InputClosed => return Err(TuiError::input_closed()),
            Msg::Input(Input::Key(chord)) => chord,
            Msg::Snapshot(snapshot) => {
                self.take_snapshot(snapshot);
                return Ok(Flow::Continue);
            }
            Msg::Input(Input::Mouse(mouse)) => {
                match mouse.kind() {
                    MouseKind::ScrollUp => self.scroll_by(-3),
                    MouseKind::ScrollDown => self.scroll_by(3),
                    _ => {}
                }
                return Ok(Flow::Continue);
            }
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
            Some(action) if matches!(self.screen, Screen::Main) => self.main_action(action),
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
            self.screen = self.landing();
        }
        self.expire(now);
        let notice = self.shown.as_ref().map(|(notice, _)| notice);
        match (&self.screen, &self.snapshot) {
            (Screen::Main, Some(snapshot)) => {
                let view = MainView {
                    snapshot,
                    selected: self.selected,
                    focus: self.focus,
                    scroll: self.scroll,
                    now: Instant::now(),
                    notice,
                };
                main::draw(frame, area, &view);
            }
            _ => home::draw(frame, area, notice, &self.caps),
        }
        if self.overlay == Overlay::Help {
            help::draw(frame, area);
        }
        colors::downsample(frame.buffer_mut(), area, self.caps.color());
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

    /// The commands queued since the last call.
    pub(crate) fn take_commands(&mut self) -> Vec<UiCommand> {
        std::mem::take(&mut self.commands)
    }

    /// How long the loop may block before the next draw; `None` blocks until input.
    pub(crate) fn next_tick(&self, now: Duration) -> Option<Duration> {
        match &self.screen {
            Screen::Splash(splash) => splash.next_tick(now).or(Some(Duration::ZERO)),
            Screen::Home | Screen::Main => {
                let notice = match (&self.shown, self.notices.is_empty()) {
                    (Some((_, until)), _) => Some(until.saturating_sub(now)),
                    (None, false) => Some(Duration::ZERO),
                    (None, true) => None,
                };
                let sending = self
                    .snapshot
                    .as_ref()
                    .is_some_and(|s| s.any_sending())
                    .then_some(SENDING_TICK);
                notice.into_iter().chain(sending).min()
            }
        }
    }
}

/// Runs the TUI until Ctrl-Q, a signal or an error; the terminal is restored on every path.
///
/// # Errors
///
/// Any `TuiError`: no terminal, a refused mode, a terminal write failure, or stopped input.
pub fn run(mut options: Options) -> Result<(), TuiError> {
    let mut term = Term::open()?;
    // Inside tmux the passthrough replies only arrive before the alternate-screen switch; elsewhere
    // the alternate screen goes first, so a terminal that prints the query payload prints it there.
    let tmux = InTmux::from_env(options.env());
    if tmux == InTmux::No {
        term.enter_screen()?;
    }
    let probed = term.probe(tmux)?;
    tracing::debug!(replies = %probed.bytes.escape_ascii(), "probe replies");
    if tmux == InTmux::Yes {
        term.enter_screen()?;
    }
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
    let snapshots = tx.clone();
    let reader = Reader::spawn(term.reader()?, tx).map_err(TuiError::terminal_io)?;
    let mut notices = Vec::new();
    let engine = Engine::start(
        options.take_dir(),
        options.settings_ref(),
        move |snapshot| {
            let _ = snapshots.send(Msg::Snapshot(snapshot));
        },
    )
    .inspect_err(|err| notices.push(Notice::from_error(err)))
    .ok();
    let backend = CrosstermBackend::new(BufWriter::with_capacity(1 << 16, term.writer()?));
    let splash = options.splash();
    let result = Terminal::new(backend)
        .map_err(TuiError::terminal_io)
        .and_then(|mut terminal| {
            notices.extend(options.into_notices());
            let mut app = App::new(splash, caps, notices);
            event_loop(&mut terminal, &mut app, &rx, engine.as_ref())
        });
    if let Some(engine) = engine {
        engine.shutdown();
    }
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
    engine: Option<&Engine>,
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
        for command in app.take_commands() {
            match (engine, command) {
                (Some(engine), UiCommand::Send(id)) => engine.send(id),
                (Some(engine), UiCommand::Cancel(id)) => engine.cancel(id),
                (None, _) => {}
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

    mod main_view {
        use std::path::{Path, PathBuf};
        use std::time::Instant;

        use pawsman_core::state::{AppState, Event};
        use pawsman_model::Collection;
        use pawsman_net::{NetEvent, Response};

        use super::*;

        const FILES: &[(&str, &str)] = &[
            (
                "a-users.req.toml",
                "name = \"List users\"\nurl = \"http://127.0.0.1:8787/users\"\n\n[headers]\nAccept = \"application/json\"\n",
            ),
            (
                "b-create.req.toml",
                "name = \"Create user\"\nmethod = \"POST\"\nurl = \"http://127.0.0.1:8787/users\"\n\n[body]\ntext = \"{\\\"name\\\":\\\"Ada\\\"}\"\n",
            ),
            ("c-broken.req.toml", "name = "),
        ];

        fn state(files: &[(&str, &str)]) -> AppState {
            let files = files
                .iter()
                .map(|(n, t)| (PathBuf::from(format!("/demo/{n}")), (*t).to_owned()))
                .collect();
            AppState::new(Collection::from_files(
                Path::new("/demo"),
                Some("format = 1\nname = \"Demo\"\n"),
                files,
            ))
        }

        fn done(state: &mut AppState, n: u32, status: u16, body: &[u8]) {
            let id = RequestId::new(n);
            state.apply(Event::Send(id), Instant::now());
            let response = Response::for_tests(status, body, Duration::from_millis(123));
            state.apply(Event::Net(NetEvent::Done { id, response }), Instant::now());
        }

        fn app_with(state: &mut AppState) -> App {
            let mut app = App::new(SplashSetting::Off, caps(), vec![]);
            app.update(Msg::Snapshot(state.snapshot()), Duration::ZERO)
                .unwrap();
            app
        }

        #[test]
        fn a_snapshot_with_a_collection_opens_the_main_view() {
            let mut app = app_with(&mut state(FILES));

            let text = screen(&render(&mut app, 160, 40, Duration::ZERO));

            assert!(
                text.contains("Requests") && text.contains("List users"),
                "main view:\n{text}"
            );
            assert!(text.contains("F5 send"), "main hints:\n{text}");
        }

        #[test]
        fn f5_queues_a_send_for_the_selected_request() {
            let mut app = app_with(&mut state(FILES));

            app.update(key(Chord::plain(KeyCode::F(5))), Duration::ZERO)
                .unwrap();

            assert_eq!(
                app.take_commands(),
                vec![UiCommand::Send(RequestId::new(1))],
                "first entry selected"
            );
            assert!(app.take_commands().is_empty(), "taken once");
        }

        #[test]
        fn down_moves_the_selection_and_tab_cycles_focus() {
            let mut app = app_with(&mut state(FILES));

            app.update(key(Chord::plain(KeyCode::Down)), Duration::ZERO)
                .unwrap();
            app.update(key(Chord::plain(KeyCode::Char('j'))), Duration::ZERO)
                .unwrap();
            app.update(key(Chord::plain(KeyCode::Char('j'))), Duration::ZERO)
                .unwrap();
            app.update(key(Chord::plain(KeyCode::F(5))), Duration::ZERO)
                .unwrap();

            assert_eq!(
                app.take_commands(),
                vec![UiCommand::Send(RequestId::new(3))],
                "stops at the last entry"
            );
            app.update(key(Chord::plain(KeyCode::Tab)), Duration::ZERO)
                .unwrap();
            app.update(key(Chord::plain(KeyCode::Up)), Duration::ZERO)
                .unwrap();
            app.update(key(Chord::plain(KeyCode::F(5))), Duration::ZERO)
                .unwrap();
            assert_eq!(
                app.take_commands(),
                vec![UiCommand::Send(RequestId::new(3))],
                "Up outside the sidebar keeps the selection"
            );
        }

        #[test]
        fn esc_cancels_only_while_sending() {
            let mut st = state(FILES);
            let mut app = app_with(&mut st);
            app.update(key(Chord::plain(KeyCode::Esc)), Duration::ZERO)
                .unwrap();
            assert!(app.take_commands().is_empty(), "idle: nothing to cancel");

            st.apply(Event::Send(RequestId::new(1)), Instant::now());
            app.update(Msg::Snapshot(st.snapshot()), Duration::ZERO)
                .unwrap();
            app.update(key(Chord::plain(KeyCode::Esc)), Duration::ZERO)
                .unwrap();

            assert_eq!(
                app.take_commands(),
                vec![UiCommand::Cancel(RequestId::new(1))],
                "sending: cancel"
            );
        }

        #[test]
        fn a_request_in_flight_ticks_every_100ms() {
            let mut st = state(FILES);
            st.apply(Event::Send(RequestId::new(1)), Instant::now());
            let mut app = app_with(&mut st);

            let text = screen(&render(&mut app, 160, 40, Duration::ZERO));

            assert!(text.contains("sending"), "elapsed shown:\n{text}");
            assert_eq!(
                app.next_tick(Duration::ZERO),
                Some(Duration::from_millis(100)),
                "ticks while sending"
            );
        }

        #[test]
        fn control_bytes_in_a_body_are_not_written_raw() {
            let mut st = state(FILES);
            done(&mut st, 1, 200, b"ok\x1b[2J\x07\x00end");
            let mut app = app_with(&mut st);

            let text = screen(&render(&mut app, 160, 40, Duration::ZERO));

            assert!(
                text.contains("ok\u{fffd}[2J\u{fffd}\u{fffd}end"),
                "escapes neutralised:\n{text}"
            );
        }

        #[test]
        fn the_selected_invalid_entry_shows_its_error() {
            let mut app = app_with(&mut state(FILES));
            for _ in 0..2 {
                app.update(key(Chord::plain(KeyCode::Down)), Duration::ZERO)
                    .unwrap();
            }

            let text = screen(&render(&mut app, 160, 40, Duration::ZERO));

            assert!(text.contains("model::invalid_file"), "error shown:\n{text}");
        }

        #[test]
        fn a_load_error_shows_on_home() {
            let mut st = AppState::new(Err(pawsman_model::ModelError::not_a_collection(
                Path::new("/x"),
            )));
            let mut app = app_with(&mut st);

            let text = screen(&render(&mut app, 100, 24, Duration::ZERO));

            assert!(
                text.contains("no collection open") && text.contains("model::not_a_collection"),
                "{text}"
            );
        }

        #[test]
        fn snapshot_160x40_done() {
            let mut st = state(FILES);
            done(&mut st, 1, 200, b"[{\"id\":1,\"name\":\"Ada\"}]");
            let mut app = app_with(&mut st);

            insta::assert_snapshot!(render(&mut app, 160, 40, Duration::ZERO).backend());
        }

        #[test]
        fn snapshot_100x30_stacked_failed() {
            let mut st = state(FILES);
            let id = RequestId::new(1);
            st.apply(Event::Send(id), Instant::now());
            let error = pawsman_net::NetError::connect(
                "127.0.0.1:8787",
                &std::io::Error::other("connection refused"),
            );
            st.apply(Event::Net(NetEvent::Failed { id, error }), Instant::now());
            let mut app = app_with(&mut st);

            insta::assert_snapshot!(render(&mut app, 100, 30, Duration::ZERO).backend());
        }

        #[test]
        fn snapshot_60x20_one_pane() {
            let mut st = state(FILES);
            done(&mut st, 1, 404, b"not found");
            let mut app = app_with(&mut st);
            app.update(key(Chord::plain(KeyCode::BackTab)), Duration::ZERO)
                .unwrap();

            insta::assert_snapshot!(render(&mut app, 60, 20, Duration::ZERO).backend());
        }

        #[test]
        fn long_body_lines_wrap_instead_of_being_cut() {
            let mut st = state(FILES);
            done(
                &mut st,
                1,
                200,
                br#"[{"id":1,"name":"Ada Lovelace"},{"id":2,"name":"Grace Hopper"}]"#,
            );
            let mut app = app_with(&mut st);
            app.update(key(Chord::plain(KeyCode::BackTab)), Duration::ZERO)
                .unwrap();

            let text = screen(&render(&mut app, 40, 20, Duration::ZERO));

            assert!(
                text.contains("Hopper"),
                "the end of the line is visible:\n{text}"
            );
        }

        #[test]
        fn a_large_body_shows_its_first_part_quickly() {
            let mut st = state(FILES);
            done(&mut st, 1, 200, &vec![b'a'; 4 * 1024 * 1024]);
            let mut app = app_with(&mut st);

            let started = std::time::Instant::now();
            let text = screen(&render(&mut app, 160, 40, Duration::ZERO));

            assert!(
                text.contains("showing the first 256.0 KB"),
                "says it is cut:\n{text}"
            );
            assert!(
                started.elapsed() < Duration::from_secs(1),
                "drew in {:?}",
                started.elapsed()
            );
        }

        #[test]
        fn tiny_main_views_never_panic() {
            let mut st = state(FILES);
            done(&mut st, 1, 200, b"x");
            for (w, h) in [(0, 0), (1, 1), (5, 2), (30, 4), (79, 10), (80, 3)] {
                let mut app = app_with(&mut st);
                render(&mut app, w, h, Duration::ZERO);
            }
        }
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
    fn home_and_help_have_no_rgb_without_truecolor() {
        for term in ["xterm-256color", "xterm"] {
            let env = EnvSnapshot::from_pairs([("TERM", term), ("LANG", "C.UTF-8")]);
            let caps = Caps::resolve(&parse(b""), &env, ImageMode::Auto);
            let mut app = App::new(SplashSetting::Off, caps, vec![Notice::warn("w")]);
            app.update(key(Chord::plain(KeyCode::F(1))), Duration::ZERO)
                .unwrap();

            let terminal = render(&mut app, 80, 24, Duration::ZERO);

            let rgb = terminal.backend().buffer().content.iter().any(|c| {
                matches!(c.fg, ratatui::style::Color::Rgb(..))
                    || matches!(c.bg, ratatui::style::Color::Rgb(..))
            });
            assert!(!rgb, "no RGB colours on {term}");
        }
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
