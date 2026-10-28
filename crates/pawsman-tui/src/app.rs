//! The main loop and its state.

use std::collections::{BTreeSet, VecDeque};
use std::io::BufWriter;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

use pawsman_core::engine::Engine;
use pawsman_core::fuzzy::Match;
use pawsman_core::keys::{Action, Context, KeyCode, Keymap, Mods};
use pawsman_core::notice::Notice;
use pawsman_core::resolve::Preview;
use pawsman_core::state::Snapshot;
use pawsman_core::tree::{Direction, NodeRef, Op};
use pawsman_model::RequestId;
use ratatui::backend::{Backend, CrosstermBackend};
use ratatui::{Frame, Terminal};
use unicode_segmentation::UnicodeSegmentation as _;

use crate::input::MouseKind;
use crate::input::{Input, Reader};
use crate::palette;
use crate::term::{Caps, InTmux, Term, probe};
use crate::tree::{self, Row};
use crate::views::main::{self, Focus, MainView, TraceView};
use crate::views::picker::{self, Item};
use crate::views::splash::{self, Splash};
use crate::views::{ask, colors, help, home, palette as palette_view, prompt};
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
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum UiCommand {
    /// Send this request.
    Send(RequestId),
    /// Cancel it.
    Cancel(RequestId),
    /// Resolve in this environment; `None` for none.
    SelectEnv(Option<String>),
    /// Change the tree on disk.
    Op(Op),
    /// Read the collection from disk again.
    Reload,
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

#[derive(Debug, Clone, PartialEq, Eq)]
enum Overlay {
    None,
    /// The help overlay, scrolled by this many rows.
    Help(u16),
    /// The environment picker, its highlighted row.
    Picker(usize),
    /// A name being typed.
    Prompt(Prompt),
    /// A yes or no question before an operation.
    Ask(Ask),
    /// The command palette: the query and the highlighted row.
    Palette {
        query: String,
        at: usize,
    },
}

/// A one-line text field in a box.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Prompt {
    title: String,
    text: String,
    purpose: PromptFor,
}

/// What a prompt's text becomes.
#[derive(Debug, Clone, PartialEq, Eq)]
enum PromptFor {
    NewRequest(PathBuf),
    NewFolder(PathBuf),
    Rename(NodeRef),
}

/// A question; yes runs `op`.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Ask {
    text: String,
    op: Op,
}

/// The longest name a prompt takes, in characters.
const PROMPT_MAX: usize = 200;

/// The selected request's preview, for one snapshot generation.
type PreviewCache = Option<(u64, RequestId, Option<Result<Preview, Notice>>)>;

/// UI state: which screen, which overlay, which notice.
pub(crate) struct App {
    screen: Screen,
    overlay: Overlay,
    caps: Caps,
    notices: VecDeque<Notice>,
    /// The current notice and when it expires, set when it is first drawn.
    shown: Option<(Notice, Duration)>,
    snapshot: Option<Arc<Snapshot>>,
    selected: Option<NodeRef>,
    collapsed: BTreeSet<PathBuf>,
    focus: Focus,
    scroll: u16,
    trace: TraceView,
    preview: PreviewCache,
    commands: Vec<UiCommand>,
    keys: Keymap,
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
            collapsed: BTreeSet::new(),
            focus: Focus::Sidebar,
            scroll: 0,
            trace: TraceView::Hidden,
            preview: None,
            commands: Vec::new(),
            keys: Keymap::default(),
        }
    }

    /// Uses `keys` instead of the default bindings.
    pub(crate) fn with_keys(mut self, keys: Keymap) -> Self {
        self.keys = keys;
        self
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
        let old_at = self
            .rows()
            .iter()
            .position(|r| Some(&r.node) == self.selected.as_ref());
        self.snapshot = Some(snapshot);
        if let Some(node) = self.created() {
            self.reveal(&node);
            self.selected = Some(node);
        }
        let rows = self.rows();
        let present = rows.iter().any(|r| Some(&r.node) == self.selected.as_ref());
        if !present {
            self.selected = match old_at {
                Some(at) => rows.get(at.min(rows.len().saturating_sub(1))),
                None => rows
                    .iter()
                    .find(|r| matches!(r.node, NodeRef::Request(_)))
                    .or_else(|| rows.first()),
            }
            .map(|r| r.node.clone());
        }
        if matches!(self.screen, Screen::Home) {
            self.screen = self.landing();
        }
    }

    /// The node a tree operation just created or renamed, from the snapshot.
    fn created(&self) -> Option<NodeRef> {
        let snapshot = self.snapshot.as_ref()?;
        tree::node_at(snapshot.collection()?, snapshot.created()?)
    }

    /// Opens every folder above `node`.
    fn reveal(&mut self, node: &NodeRef) {
        let Some(collection) = self.snapshot.as_ref().and_then(|s| s.collection()) else {
            return;
        };
        let parent = tree::parent(collection, node);
        for dir in parent.ancestors() {
            self.collapsed.remove(dir);
        }
    }

    /// The visible sidebar rows.
    fn rows(&self) -> Vec<Row> {
        self.snapshot
            .as_ref()
            .and_then(|s| s.collection())
            .map(|c| tree::rows(c, &self.collapsed))
            .unwrap_or_default()
    }

    /// The selected request, when a request (not a folder) is selected.
    fn selected_id(&self) -> Option<RequestId> {
        match self.selected {
            Some(NodeRef::Request(id)) => Some(id),
            _ => None,
        }
    }

    /// Moves the selection by `step` rows, clamped to the list.
    fn select(&mut self, step: isize) {
        let rows = self.rows();
        let current = rows
            .iter()
            .position(|r| Some(&r.node) == self.selected.as_ref())
            .unwrap_or(0);
        let next = current
            .saturating_add_signed(step)
            .min(rows.len().saturating_sub(1));
        if let Some(row) = rows.get(next) {
            self.selected = Some(row.node.clone());
        }
        self.scroll = 0;
    }

    /// Closes an open folder, else selects the parent folder.
    fn collapse(&mut self) {
        let Some(node) = self.selected.clone() else {
            return;
        };
        if let NodeRef::Folder(path) = &node
            && !self.collapsed.contains(path)
        {
            self.collapsed.insert(path.clone());
            return;
        }
        let Some(collection) = self.snapshot.as_ref().and_then(|s| s.collection()) else {
            return;
        };
        let parent = tree::parent(collection, &node);
        if !parent.as_os_str().is_empty() {
            self.selected = Some(NodeRef::Folder(parent));
        }
    }

    /// Opens or closes the selected folder.
    fn toggle(&mut self, open: Option<bool>) {
        let Some(NodeRef::Folder(path)) = &self.selected else {
            return;
        };
        let open = open.unwrap_or_else(|| self.collapsed.contains(path));
        if open {
            self.collapsed.remove(path);
        } else {
            self.collapsed.insert(path.clone());
        }
    }

    /// The display name of a node and the folder new nodes go into.
    fn describe(&self, node: &NodeRef) -> Option<(String, PathBuf)> {
        let collection = self.snapshot.as_ref()?.collection()?;
        match node {
            NodeRef::Request(id) => {
                let entry = collection.entry(*id)?;
                let name = collection
                    .request(*id)
                    .map_or_else(|| entry.stem().to_owned(), |r| r.name().to_owned());
                Some((name, entry.folder().to_owned()))
            }
            NodeRef::Folder(path) => {
                let folder = tree::folder(collection, path)?;
                Some((folder.name().to_owned(), path.clone()))
            }
        }
    }

    /// The folder a new node goes into: the selected folder, or the selected request's folder.
    fn target_folder(&self) -> (String, PathBuf) {
        let collection = self.snapshot.as_ref().and_then(|s| s.collection());
        let path = self
            .selected
            .as_ref()
            .and_then(|n| self.describe(n))
            .map(|(_, p)| p)
            .unwrap_or_default();
        let name = collection
            .and_then(|c| tree::folder(c, &path))
            .map(|f| f.name().to_owned())
            .unwrap_or_default();
        (name, path)
    }

    fn tree_action(&mut self, action: Action) {
        let node = self.selected.clone();
        match action {
            Action::NewRequest | Action::NewFolder => {
                let (name, parent) = self.target_folder();
                let (title, purpose) = if action == Action::NewRequest {
                    (
                        format!("New request in {name}"),
                        PromptFor::NewRequest(parent),
                    )
                } else {
                    (
                        format!("New folder in {name}"),
                        PromptFor::NewFolder(parent),
                    )
                };
                self.overlay = Overlay::Prompt(Prompt {
                    title,
                    text: String::new(),
                    purpose,
                });
            }
            Action::Rename => {
                let Some(node) = node else {
                    return;
                };
                let Some((name, _)) = self.describe(&node) else {
                    return;
                };
                self.overlay = Overlay::Prompt(Prompt {
                    title: format!("Rename {name}"),
                    text: name,
                    purpose: PromptFor::Rename(node),
                });
            }
            Action::Delete => {
                let Some(node) = node else {
                    return;
                };
                let Some((name, _)) = self.describe(&node) else {
                    return;
                };
                let text = match &node {
                    NodeRef::Request(_) => format!("Delete request {name}?"),
                    NodeRef::Folder(path) => {
                        let n = self
                            .snapshot
                            .as_ref()
                            .and_then(|s| s.collection())
                            .and_then(|c| tree::folder(c, path))
                            .map_or(0, tree::count);
                        let noun = if n == 1 { "request" } else { "requests" };
                        format!("Delete folder {name} and its {n} {noun}?")
                    }
                };
                self.overlay = Overlay::Ask(Ask {
                    text,
                    op: Op::Delete(node),
                });
            }
            Action::MoveUp | Action::MoveDown => {
                let dir = if action == Action::MoveUp {
                    Direction::Up
                } else {
                    Direction::Down
                };
                if let Some(node) = node {
                    self.commands.push(UiCommand::Op(Op::Move { node, dir }));
                }
            }
            Action::Reload => self.commands.push(UiCommand::Reload),
            Action::Expand => self.toggle(Some(true)),
            Action::Collapse => self.collapse(),
            Action::Confirm => self.toggle(None),
            _ => {}
        }
    }

    fn prompt_action(&mut self, action: Action, prompt: Prompt) {
        if action != Action::Confirm {
            return;
        }
        let name = prompt.text.trim().to_owned();
        if name.is_empty() {
            return;
        }
        let op = match prompt.purpose {
            PromptFor::NewRequest(parent) => Op::NewRequest { parent, name },
            PromptFor::NewFolder(parent) => Op::NewFolder { parent, name },
            PromptFor::Rename(node) => Op::Rename { node, name },
        };
        self.commands.push(UiCommand::Op(op));
        self.overlay = Overlay::None;
    }

    /// Typing into the prompt or the palette query: characters, Backspace, paste. `false` when
    /// no text field is open or `input` is not text.
    fn type_into_prompt(&mut self, input: &Input) -> bool {
        let field = match &mut self.overlay {
            Overlay::Prompt(prompt) => &mut prompt.text,
            Overlay::Palette { query, at } => {
                *at = 0;
                query
            }
            _ => return false,
        };
        let text = match input {
            Input::Key(chord)
                if (chord.mods().is_empty() || chord.mods() == Mods::SHIFT)
                    && let KeyCode::Char(c) = chord.code() =>
            {
                c.to_string()
            }
            Input::Key(chord) if chord.mods().is_empty() && chord.code() == KeyCode::Backspace => {
                if let Some((at, _)) = field.grapheme_indices(true).next_back() {
                    field.truncate(at);
                }
                return true;
            }
            Input::Paste(text) => text.chars().filter(|c| !c.is_control()).collect(),
            _ => return false,
        };
        let room = PROMPT_MAX.saturating_sub(field.chars().count());
        field.extend(text.chars().take(room));
        true
    }

    /// The notice for Ctrl-C with nothing selected.
    fn copy_hint(&mut self) {
        self.notices.push_back(Notice::info(format!(
            "nothing selected · {} quits",
            self.keys.label(Action::Quit).unwrap_or_default()
        )));
    }

    /// The palette's items for `query`, best first.
    fn palette_items(&self, query: &str) -> Vec<(palette::Item, Match)> {
        let Some(snapshot) = &self.snapshot else {
            return Vec::new();
        };
        palette::filter(palette::items(snapshot, &self.keys), query)
    }

    /// Up, Down and Enter in the palette; returns the action to run after it closes.
    fn palette_action(&mut self, action: Action, query: &str, at: usize) -> Option<Action> {
        let items = self.palette_items(query);
        match action {
            Action::Up | Action::Down => {
                let next = if action == Action::Up {
                    at.saturating_sub(1)
                } else {
                    (at + 1).min(items.len().saturating_sub(1))
                };
                self.overlay = Overlay::Palette {
                    query: query.to_owned(),
                    at: next,
                };
                None
            }
            Action::Confirm => {
                let (item, _) = items.into_iter().nth(at)?;
                self.overlay = Overlay::None;
                match item {
                    palette::Item::Request { id, .. } => {
                        let node = NodeRef::Request(id);
                        self.reveal(&node);
                        self.selected = Some(node);
                        self.focus = Focus::Sidebar;
                        self.scroll = 0;
                        None
                    }
                    palette::Item::Action { action, .. } => Some(action),
                }
            }
            _ => None,
        }
    }

    fn main_action(&mut self, action: Action) {
        match action {
            Action::Send => {
                if let Some(id) = self.selected_id() {
                    self.commands.push(UiCommand::Send(id));
                }
            }
            Action::Cancel => {
                let sending =
                    self.snapshot
                        .as_ref()
                        .zip(self.selected_id())
                        .is_some_and(|(s, id)| {
                            matches!(
                                s.status(id),
                                pawsman_core::state::RequestStatus::Sending { .. }
                            )
                        });
                if sending && let Some(id) = self.selected_id() {
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
            Action::PickEnv => {
                let active = self.snapshot.as_ref().and_then(|s| s.active_env());
                let at = self
                    .picker_items()
                    .iter()
                    .position(|i| i.env.as_deref() == active)
                    .unwrap_or(0);
                self.overlay = Overlay::Picker(at);
            }
            Action::Trace => self.trace = self.trace.toggled(),
            Action::Palette => {
                self.overlay = Overlay::Palette {
                    query: String::new(),
                    at: 0,
                };
            }
            action => self.tree_action(action),
        }
    }

    /// `none`, then the collection's environments.
    fn picker_items(&self) -> Vec<Item> {
        let envs = self.snapshot.as_ref().map_or(&[][..], |s| s.environments());
        std::iter::once(Item {
            env: None,
            valid: true,
        })
        .chain(envs.iter().map(|e| Item {
            env: Some(e.name().to_owned()),
            valid: e.error().is_none(),
        }))
        .collect()
    }

    fn picker_action(&mut self, action: Action, at: usize) {
        let items = self.picker_items();
        match action {
            Action::Up | Action::Down => {
                let valid: Vec<usize> = (0..items.len())
                    .filter(|i| items.get(*i).is_some_and(|item| item.valid))
                    .collect();
                let next = if action == Action::Up {
                    valid.iter().rev().find(|i| **i < at)
                } else {
                    valid.iter().find(|i| **i > at)
                };
                self.overlay = Overlay::Picker(next.copied().unwrap_or(at));
            }
            Action::Confirm => {
                if let Some(item) = items.get(at).filter(|item| item.valid) {
                    self.commands.push(UiCommand::SelectEnv(item.env.clone()));
                    self.overlay = Overlay::None;
                }
            }
            _ => {}
        }
    }

    /// Recomputes the selected request's preview once per snapshot generation.
    fn refresh_preview(&mut self) {
        let (Some(snapshot), Some(id)) = (self.snapshot.as_ref(), self.selected_id()) else {
            return;
        };
        let generation = snapshot.generation();
        let fresh = self
            .preview
            .as_ref()
            .is_some_and(|(g, i, _)| *g == generation && *i == id);
        if !fresh {
            self.preview = Some((generation, id, snapshot.preview(id)));
        }
    }

    fn scroll_by(&mut self, step: i32) {
        self.scroll = u16::try_from((i32::from(self.scroll) + step).max(0)).unwrap_or(u16::MAX);
    }

    fn context(&self) -> Context {
        match (&self.screen, &self.overlay) {
            (Screen::Splash(_), _) => Context::Splash,
            (Screen::Home | Screen::Main, Overlay::Help(_)) => Context::Help,
            (Screen::Main, Overlay::Picker(_)) => Context::Picker,
            (Screen::Main, Overlay::Prompt(_)) => Context::Prompt,
            (Screen::Main, Overlay::Ask(_)) => Context::Ask,
            (Screen::Main, Overlay::Palette { .. }) => Context::Palette,
            (Screen::Home, _) => Context::Home,
            (Screen::Main, Overlay::None) => Context::Main,
        }
    }

    /// Applies one message.
    pub(crate) fn update(&mut self, msg: Msg, _now: Duration) -> Result<Flow, TuiError> {
        if let Msg::Input(input) = &msg
            && self.type_into_prompt(input)
        {
            return Ok(Flow::Continue);
        }
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
        let action = self.keys.lookup(self.context(), chord);
        if let Screen::Splash(splash) = &mut self.screen
            && action != Some(Action::Quit)
        {
            splash.skip();
            return Ok(Flow::Continue);
        }
        match action {
            Some(Action::Quit) => return Ok(Flow::Quit),
            Some(Action::Help) => self.overlay = Overlay::Help(0),
            Some(Action::Close) => self.overlay = Overlay::None,
            Some(action) if let Overlay::Help(scroll) = self.overlay => {
                let step: i32 = match action {
                    Action::Up => -1,
                    Action::Down => 1,
                    Action::PageUp => -i32::from(PAGE),
                    Action::PageDown => i32::from(PAGE),
                    _ => 0,
                };
                let next = (i32::from(scroll) + step).max(0);
                self.overlay = Overlay::Help(u16::try_from(next).unwrap_or(u16::MAX));
            }
            Some(action) if let Overlay::Picker(at) = self.overlay => {
                self.picker_action(action, at);
            }
            Some(action) if let Overlay::Prompt(prompt) = &self.overlay => {
                self.prompt_action(action, prompt.clone());
            }
            Some(action) if let Overlay::Palette { query, at } = &self.overlay => {
                let (query, at) = (query.clone(), *at);
                match self.palette_action(action, &query, at) {
                    Some(Action::Quit) => return Ok(Flow::Quit),
                    Some(Action::Help) => self.overlay = Overlay::Help(0),
                    Some(Action::Copy) => self.copy_hint(),
                    Some(action) => self.main_action(action),
                    None => {}
                }
            }
            Some(Action::Confirm) if let Overlay::Ask(ask) = &self.overlay => {
                self.commands.push(UiCommand::Op(ask.op.clone()));
                self.overlay = Overlay::None;
            }
            Some(_) if matches!(self.overlay, Overlay::Ask(_)) => {}
            Some(Action::Copy) => self.copy_hint(),
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
        let notice = notice.cloned();
        let on_main = matches!(self.screen, Screen::Main) && self.snapshot.is_some();
        if on_main {
            self.refresh_preview();
            let preview = self
                .preview
                .as_ref()
                .filter(|(_, id, _)| Some(*id) == self.selected_id())
                .and_then(|(_, _, p)| p.as_ref());
            let rows = self.rows();
            if let Some(snapshot) = &self.snapshot {
                let view = MainView {
                    snapshot,
                    selected: self.selected_id(),
                    node: self.selected.as_ref(),
                    rows: &rows,
                    collapsed: &self.collapsed,
                    focus: self.focus,
                    scroll: self.scroll,
                    now: Instant::now(),
                    notice: notice.as_ref(),
                    preview,
                    trace: self.trace,
                    keys: &self.keys,
                };
                main::draw(frame, area, &view);
            }
        } else {
            home::draw(frame, area, notice.as_ref(), &self.caps, &self.keys);
        }
        match &self.overlay {
            Overlay::Help(scroll) => {
                self.overlay = Overlay::Help(help::draw(frame, area, &self.keys, *scroll));
            }
            Overlay::Picker(at) => picker::draw(frame, area, &self.picker_items(), *at),
            Overlay::Prompt(p) => prompt::draw(frame, area, &p.title, &p.text),
            Overlay::Ask(a) => ask::draw(frame, area, &a.text),
            Overlay::Palette { query, at } => {
                let items = self.palette_items(query);
                palette_view::draw(frame, area, query, &items, *at);
            }
            Overlay::None => {}
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
    let engine = Engine::start(options.engine_setup(), move |snapshot| {
        let _ = snapshots.send(Msg::Snapshot(snapshot));
    })
    .inspect_err(|err| {
        tracing::warn!(%err, "engine did not start");
        notices.push(Notice::from_error(err));
    })
    .ok();
    let backend = CrosstermBackend::new(BufWriter::with_capacity(1 << 16, term.writer()?));
    let splash = options.splash();
    let keys = options.keys().clone();
    let result = Terminal::new(backend)
        .map_err(TuiError::terminal_io)
        .and_then(|mut terminal| {
            notices.extend(options.into_notices());
            let mut app = App::new(splash, caps, notices).with_keys(keys);
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
                (Some(engine), UiCommand::SelectEnv(env)) => engine.select_env(env),
                (Some(engine), UiCommand::Op(op)) => engine.apply(op),
                (Some(engine), UiCommand::Reload) => engine.reload(),
                (None, _) => {}
            }
        }
    }
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
        use pawsman_core::tree::{Direction, NodeRef};
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
                .map(|(n, t)| {
                    pawsman_model::SourceFile::new(PathBuf::from(n), t.as_bytes().to_vec())
                })
                .chain(std::iter::once(pawsman_model::SourceFile::new(
                    "pawsman.toml".into(),
                    b"format = 1\nname = \"Demo\"\n".to_vec(),
                )))
                .collect();
            AppState::new(
                Collection::from_files(Path::new("/demo"), files),
                pawsman_core::resolve::Context::default(),
            )
        }

        fn done(state: &mut AppState, n: u32, status: u16, body: &[u8]) {
            let id = RequestId::new(n);
            state.apply(
                Event::Send {
                    id,
                    at: std::time::UNIX_EPOCH,
                    seed: 1,
                },
                Instant::now(),
            );
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

            st.apply(
                Event::Send {
                    id: RequestId::new(1),
                    at: std::time::UNIX_EPOCH,
                    seed: 1,
                },
                Instant::now(),
            );
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
            st.apply(
                Event::Send {
                    id: RequestId::new(1),
                    at: std::time::UNIX_EPOCH,
                    seed: 1,
                },
                Instant::now(),
            );
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
            let mut st = AppState::new(
                Err(pawsman_model::ModelError::not_a_collection(Path::new("/x"))),
                pawsman_core::resolve::Context::default(),
            );
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
            st.apply(
                Event::Send {
                    id,
                    at: std::time::UNIX_EPOCH,
                    seed: 1,
                },
                Instant::now(),
            );
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

        const TREE: &[(&str, &str)] = &[
            ("a/one.req.toml", "name = \"One\"\nurl = \"http://h/1\"\n"),
            ("a/_folder.toml", "name = \"Alpha\"\n"),
            ("b/two.req.toml", "name = \"Two\"\nurl = \"{{base}}/2\"\n"),
            ("b/_folder.toml", "title = \"broken\"\n"),
            ("top.req.toml", "name = \"Top\"\nurl = \"http://h/top\"\n"),
        ];

        const ENVS: &[(&str, &str)] = &[
            (
                "me.req.toml",
                "name = \"Me\"\nurl = \"{{base}}/me\"\n[headers]\nAuthorization = \"Bearer {{token}}\"\n",
            ),
            (
                "envs/dev.env.toml",
                "secrets = [\"token\"]\n[vars]\nbase = \"http://dev\"\n",
            ),
            ("envs/prod.env.toml", "[vars]\nbase = \"http://prod\"\n"),
            ("envs/broken.env.toml", "vars = 1\n"),
        ];

        fn state_with(files: &[(&str, &str)], env: &[(&str, &str)]) -> AppState {
            let files = files
                .iter()
                .map(|(n, t)| {
                    pawsman_model::SourceFile::new(PathBuf::from(n), t.as_bytes().to_vec())
                })
                .chain(std::iter::once(pawsman_model::SourceFile::new(
                    "pawsman.toml".into(),
                    b"format = 1\nname = \"Demo\"\n".to_vec(),
                )))
                .collect();
            AppState::new(
                Collection::from_files(Path::new("/demo"), files),
                pawsman_core::resolve::Context::new(
                    vec![],
                    EnvSnapshot::from_pairs(env.iter().copied()),
                ),
            )
        }

        fn char_key(c: char) -> Msg {
            key(Chord::plain(KeyCode::Char(c)))
        }

        fn press(app: &mut App, msgs: impl IntoIterator<Item = Msg>) {
            for msg in msgs {
                app.update(msg, Duration::ZERO).unwrap();
            }
        }

        fn dev(st: &mut AppState) -> App {
            st.apply(Event::SelectEnv(Some("dev".into())), Instant::now());
            app_with(st)
        }

        fn keys(app: &mut App, codes: &[KeyCode]) {
            press(app, codes.iter().map(|c| key(Chord::plain(*c))));
        }

        fn typed(app: &mut App, text: &str) {
            press(app, text.chars().map(char_key));
        }

        fn sends(app: &mut App) -> Vec<UiCommand> {
            press(app, [key(Chord::plain(KeyCode::F(5)))]);
            app.take_commands()
        }

        #[test]
        fn down_visits_folder_rows() {
            let mut app = app_with(&mut state(TREE));

            keys(&mut app, &[KeyCode::Down]);
            let on_folder = sends(&mut app);
            keys(&mut app, &[KeyCode::Down]);
            let on_two = sends(&mut app);

            assert!(on_folder.is_empty(), "the `b` folder row: {on_folder:?}");
            assert_eq!(
                on_two,
                vec![UiCommand::Send(RequestId::new(2))],
                "then b/two"
            );
        }

        #[test]
        fn left_collapses_then_goes_to_the_parent() {
            let mut app = app_with(&mut state(TREE));

            keys(&mut app, &[KeyCode::Left]);
            let on_folder = sends(&mut app);
            keys(&mut app, &[KeyCode::Left]);
            let text = screen(&render(&mut app, 160, 40, Duration::ZERO));
            keys(&mut app, &[KeyCode::Down, KeyCode::Down]);

            assert!(on_folder.is_empty(), "on the `a` folder: {on_folder:?}");
            assert!(!text.contains("One"), "collapsed:\n{text}");
            assert_eq!(
                sends(&mut app),
                vec![UiCommand::Send(RequestId::new(2))],
                "a (collapsed) → b → two"
            );
        }

        #[test]
        fn enter_toggles_a_folder() {
            let mut app = app_with(&mut state(TREE));
            keys(&mut app, &[KeyCode::Up, KeyCode::Enter]);
            let closed = screen(&render(&mut app, 160, 40, Duration::ZERO));
            keys(&mut app, &[KeyCode::Enter]);
            let open = screen(&render(&mut app, 160, 40, Duration::ZERO));

            assert!(!closed.contains("One"), "closed:\n{closed}");
            assert!(open.contains("One"), "open:\n{open}");
            assert!(app.take_commands().is_empty(), "no command");
        }

        #[test]
        fn a_opens_a_prompt_and_enter_queues_new_request_in_the_folder() {
            let mut app = app_with(&mut state(TREE));

            press(&mut app, [char_key('a')]);
            let text = screen(&render(&mut app, 160, 40, Duration::ZERO));
            typed(&mut app, "Get x");
            keys(&mut app, &[KeyCode::Enter]);

            assert!(text.contains("New request in Alpha"), "title:\n{text}");
            assert_eq!(
                app.take_commands(),
                vec![UiCommand::Op(Op::NewRequest {
                    parent: "a".into(),
                    name: "Get x".into()
                })]
            );
        }

        #[test]
        fn shift_a_makes_a_folder_and_a_blank_name_does_nothing() {
            let mut app = app_with(&mut state(TREE));

            press(&mut app, [char_key('A')]);
            typed(&mut app, "  ");
            keys(&mut app, &[KeyCode::Enter, KeyCode::Esc]);
            press(&mut app, [char_key('A')]);
            typed(&mut app, "Sub");
            keys(&mut app, &[KeyCode::Enter]);

            assert_eq!(
                app.take_commands(),
                vec![UiCommand::Op(Op::NewFolder {
                    parent: "a".into(),
                    name: "Sub".into()
                })]
            );
        }

        #[test]
        fn letters_type_in_the_prompt_even_when_bound() {
            let mut app = app_with(&mut state_with(ENVS, &[]));

            press(&mut app, [char_key('a')]);
            typed(&mut app, "etjk");
            let text = screen(&render(&mut app, 160, 40, Duration::ZERO));
            keys(&mut app, &[KeyCode::Enter]);

            assert!(!text.contains("Environment"), "no picker:\n{text}");
            assert!(text.contains("etjk"), "typed:\n{text}");
            assert_eq!(
                app.take_commands(),
                vec![UiCommand::Op(Op::NewRequest {
                    parent: "".into(),
                    name: "etjk".into()
                })]
            );
        }

        #[test]
        fn a_paste_types_into_the_prompt_without_newlines() {
            let mut app = app_with(&mut state(TREE));

            press(
                &mut app,
                [
                    char_key('a'),
                    Msg::Input(Input::Paste("Get\nuser\x1b".into())),
                ],
            );
            keys(&mut app, &[KeyCode::Enter]);

            assert_eq!(
                app.take_commands(),
                vec![UiCommand::Op(Op::NewRequest {
                    parent: "a".into(),
                    name: "Getuser".into()
                })]
            );
        }

        #[test]
        fn ctrl_q_quits_from_the_prompt() {
            let mut app = app_with(&mut state(TREE));
            press(&mut app, [char_key('a')]);

            let flow = app.update(key(Chord::ctrl('q')), Duration::ZERO).unwrap();

            assert_eq!(flow, Flow::Quit, "quit");
        }

        #[test]
        fn rename_prefills_the_name() {
            let mut app = app_with(&mut state(TREE));

            press(&mut app, [char_key('r')]);
            let text = screen(&render(&mut app, 160, 40, Duration::ZERO));
            keys(&mut app, &[KeyCode::Backspace; 3]);
            typed(&mut app, "Uno");
            keys(&mut app, &[KeyCode::Enter]);

            assert!(text.contains("Rename One"), "title:\n{text}");
            assert_eq!(
                app.take_commands(),
                vec![UiCommand::Op(Op::Rename {
                    node: NodeRef::Request(RequestId::new(1)),
                    name: "Uno".into()
                })]
            );
        }

        #[test]
        fn delete_asks_and_y_confirms() {
            let mut app = app_with(&mut state(TREE));

            press(&mut app, [char_key('d')]);
            let text = screen(&render(&mut app, 160, 40, Duration::ZERO));
            press(&mut app, [char_key('y')]);

            assert!(text.contains("Delete request One?"), "asked:\n{text}");
            assert_eq!(
                app.take_commands(),
                vec![UiCommand::Op(Op::Delete(NodeRef::Request(RequestId::new(
                    1
                ))))]
            );
        }

        #[test]
        fn deleting_a_folder_names_its_requests() {
            let mut app = app_with(&mut state(TREE));

            keys(&mut app, &[KeyCode::Up]);
            press(&mut app, [char_key('d')]);
            let text = screen(&render(&mut app, 160, 40, Duration::ZERO));

            assert!(
                text.contains("Delete folder Alpha and its 1 request?"),
                "{text}"
            );
        }

        #[test]
        fn esc_and_n_cancel_the_ask() {
            let mut app = app_with(&mut state(TREE));

            press(&mut app, [char_key('d'), key(Chord::plain(KeyCode::Esc))]);
            press(&mut app, [char_key('d'), char_key('n')]);

            assert!(app.take_commands().is_empty(), "nothing deleted");
        }

        #[test]
        fn shift_k_and_j_move_and_ctrl_r_reloads() {
            let mut app = app_with(&mut state(TREE));

            press(
                &mut app,
                [char_key('K'), char_key('J'), key(Chord::ctrl('r'))],
            );

            let node = NodeRef::Request(RequestId::new(1));
            assert_eq!(
                app.take_commands(),
                vec![
                    UiCommand::Op(Op::Move {
                        node: node.clone(),
                        dir: Direction::Up
                    }),
                    UiCommand::Op(Op::Move {
                        node,
                        dir: Direction::Down
                    }),
                    UiCommand::Reload,
                ]
            );
        }

        #[test]
        fn a_created_path_is_selected_and_its_folder_expanded() {
            let mut st = state(TREE);
            let mut app = app_with(&mut st);
            keys(
                &mut app,
                &[KeyCode::Up, KeyCode::Left, KeyCode::Down, KeyCode::Down],
            );

            st.apply(Event::Created("a/one.req.toml".into()), Instant::now());
            app.update(Msg::Snapshot(st.snapshot()), Duration::ZERO)
                .unwrap();
            let text = screen(&render(&mut app, 160, 40, Duration::ZERO));

            assert!(text.contains("One"), "expanded:\n{text}");
            assert_eq!(sends(&mut app), vec![UiCommand::Send(RequestId::new(1))]);
        }

        #[test]
        fn the_selection_follows_its_id_across_a_reload() {
            let mut ids = pawsman_model::Ids::default();
            let build = |extra: &[(&str, &str)], ids: &mut pawsman_model::Ids| {
                let files = TREE
                    .iter()
                    .chain(extra)
                    .chain(&[("pawsman.toml", "format = 1\nname = \"Demo\"\n")])
                    .map(|(n, t)| {
                        pawsman_model::SourceFile::new(PathBuf::from(n), t.as_bytes().to_vec())
                    })
                    .collect();
                Collection::build(Path::new("/demo"), files, ids)
            };
            let mut st = AppState::new(
                build(&[], &mut ids),
                pawsman_core::resolve::Context::default(),
            );
            let mut app = app_with(&mut st);
            keys(&mut app, &[KeyCode::Down, KeyCode::Down]);

            st.apply(
                Event::Reloaded(Box::new(build(
                    &[("a/aaa.req.toml", "name = \"Aaa\"\nurl = \"u\"\n")],
                    &mut ids,
                ))),
                Instant::now(),
            );
            app.update(Msg::Snapshot(st.snapshot()), Duration::ZERO)
                .unwrap();

            assert_eq!(
                sends(&mut app),
                vec![UiCommand::Send(RequestId::new(2))],
                "still two"
            );
        }

        #[test]
        fn a_folder_shows_its_summary() {
            let mut app = app_with(&mut state(TREE));
            keys(&mut app, &[KeyCode::Up]);

            let text = screen(&render(&mut app, 160, 40, Duration::ZERO));

            assert!(
                text.contains("folder Alpha") && text.contains("1 request"),
                "{text}"
            );
        }

        fn palette(app: &mut App, query: &str) {
            press(app, [key(Chord::ctrl('k'))]);
            typed(app, query);
        }

        #[test]
        fn ctrl_k_opens_and_a_query_without_matches_says_so() {
            let mut app = app_with(&mut state(TREE));

            press(&mut app, [key(Chord::ctrl('k'))]);
            let open = screen(&render(&mut app, 160, 40, Duration::ZERO));
            typed(&mut app, "xyz");
            let none = screen(&render(&mut app, 160, 40, Duration::ZERO));

            assert!(open.contains("› ") && open.contains("Send"), "{open}");
            assert!(none.contains("no match"), "{none}");
        }

        #[test]
        fn j_and_k_type_in_the_palette() {
            let mut app = app_with(&mut state(TREE));

            palette(&mut app, "jk");
            let text = screen(&render(&mut app, 160, 40, Duration::ZERO));

            assert!(text.contains("› jk"), "{text}");
        }

        #[test]
        fn enter_on_a_request_selects_it() {
            let mut app = app_with(&mut state(TREE));

            palette(&mut app, "top");
            keys(&mut app, &[KeyCode::Enter]);

            assert_eq!(sends(&mut app), vec![UiCommand::Send(RequestId::new(3))]);
        }

        #[test]
        fn a_request_in_a_collapsed_folder_is_revealed() {
            let mut app = app_with(&mut state(TREE));
            keys(&mut app, &[KeyCode::Up, KeyCode::Left, KeyCode::Down]);

            palette(&mut app, "one");
            keys(&mut app, &[KeyCode::Enter]);
            let text = screen(&render(&mut app, 160, 40, Duration::ZERO));

            assert!(text.contains("One"), "expanded:\n{text}");
            assert_eq!(sends(&mut app), vec![UiCommand::Send(RequestId::new(1))]);
        }

        #[test]
        fn down_moves_the_highlight() {
            let mut app = app_with(&mut state(TREE));

            palette(&mut app, "o");
            keys(
                &mut app,
                &[KeyCode::Down, KeyCode::Up, KeyCode::Down, KeyCode::Enter],
            );

            assert!(
                matches!(app.overlay, Overlay::None),
                "chosen: {:?}",
                app.overlay
            );
        }

        #[test]
        fn enter_on_an_action_runs_it() {
            let mut app = app_with(&mut state_with(ENVS, &[]));

            palette(&mut app, "pick env");
            keys(&mut app, &[KeyCode::Enter]);
            let text = screen(&render(&mut app, 160, 40, Duration::ZERO));

            assert!(text.contains("Environment"), "picker open:\n{text}");
        }

        #[test]
        fn esc_closes_the_palette() {
            let mut app = app_with(&mut state(TREE));

            palette(&mut app, "o");
            keys(&mut app, &[KeyCode::Esc]);
            let text = screen(&render(&mut app, 160, 40, Duration::ZERO));

            assert!(!text.contains("› o"), "{text}");
            assert!(app.take_commands().is_empty(), "nothing run");
        }

        #[test]
        fn snapshot_100x30_palette() {
            let mut app = app_with(&mut state(TREE));
            palette(&mut app, "o");

            insta::assert_snapshot!(render(&mut app, 100, 30, Duration::ZERO).backend());
        }

        #[test]
        fn tiny_palettes_never_panic() {
            for (w, h) in [(0, 0), (1, 1), (5, 2), (30, 4), (44, 6)] {
                let mut app = app_with(&mut state(TREE));
                palette(&mut app, "o");
                render(&mut app, w, h, Duration::ZERO);
            }
        }

        #[test]
        fn snapshot_160x40_tree_collapsed() {
            let mut app = app_with(&mut state(TREE));
            keys(&mut app, &[KeyCode::Up, KeyCode::Left]);

            insta::assert_snapshot!(render(&mut app, 160, 40, Duration::ZERO).backend());
        }

        #[test]
        fn snapshot_80x24_prompt() {
            let mut app = app_with(&mut state(TREE));
            press(&mut app, [char_key('r')]);

            insta::assert_snapshot!(render(&mut app, 80, 24, Duration::ZERO).backend());
        }

        #[test]
        fn snapshot_80x24_ask() {
            let mut app = app_with(&mut state(TREE));
            keys(&mut app, &[KeyCode::Up]);
            press(&mut app, [char_key('d')]);

            insta::assert_snapshot!(render(&mut app, 80, 24, Duration::ZERO).backend());
        }

        #[test]
        fn enter_in_the_picker_queues_select_env() {
            let mut app = app_with(&mut state_with(ENVS, &[]));

            press(
                &mut app,
                [
                    char_key('e'),
                    key(Chord::plain(KeyCode::Down)),
                    key(Chord::plain(KeyCode::Enter)),
                ],
            );

            assert_eq!(
                app.take_commands(),
                vec![UiCommand::SelectEnv(Some("dev".into()))],
                "none → dev: the highlight skips the broken one, which sorts first"
            );
        }

        #[test]
        fn esc_closes_the_picker_without_a_command() {
            let mut app = app_with(&mut state_with(ENVS, &[]));

            press(&mut app, [char_key('e'), key(Chord::plain(KeyCode::Esc))]);

            assert!(app.take_commands().is_empty(), "nothing chosen");
            let text = screen(&render(&mut app, 160, 40, Duration::ZERO));
            assert!(!text.contains("(invalid)"), "picker closed:\n{text}");
        }

        #[test]
        fn a_secret_is_never_drawn() {
            let mut st = state_with(ENVS, &[("PAWSMAN_SECRET_TOKEN", "s3cr3t")]);
            let mut app = dev(&mut st);
            press(&mut app, [char_key('t')]);

            let text = screen(&render(&mut app, 160, 40, Duration::ZERO));

            assert!(!text.contains("s3cr3t"), "masked:\n{text}");
            assert!(text.contains("Bearer ••••••"), "mask shown:\n{text}");
            assert!(text.contains("http://dev/me"), "resolved url:\n{text}");
        }

        #[test]
        fn snapshot_160x40_tree() {
            let mut app = app_with(&mut state(TREE));

            insta::assert_snapshot!(render(&mut app, 160, 40, Duration::ZERO).backend());
        }

        #[test]
        fn snapshot_160x40_picker_with_a_broken_env() {
            let mut app = app_with(&mut state_with(ENVS, &[]));
            press(&mut app, [char_key('e')]);

            insta::assert_snapshot!(render(&mut app, 160, 40, Duration::ZERO).backend());
        }

        #[test]
        fn snapshot_160x40_trace_with_a_secret() {
            let mut st = state_with(ENVS, &[("PAWSMAN_SECRET_TOKEN", "s3cr3t")]);
            let mut app = dev(&mut st);
            press(&mut app, [char_key('t')]);

            insta::assert_snapshot!(render(&mut app, 160, 40, Duration::ZERO).backend());
        }

        #[test]
        fn snapshot_160x40_unresolved() {
            let mut st = state_with(ENVS, &[]);
            st.apply(
                Event::Send {
                    id: RequestId::new(1),
                    at: std::time::UNIX_EPOCH,
                    seed: 1,
                },
                Instant::now(),
            );
            let mut app = app_with(&mut st);

            insta::assert_snapshot!(render(&mut app, 160, 40, Duration::ZERO).backend());
        }

        #[test]
        fn snapshot_60x20_sidebar_with_env_row() {
            let mut st = state_with(ENVS, &[]);
            let mut app = dev(&mut st);

            insta::assert_snapshot!(render(&mut app, 60, 20, Duration::ZERO).backend());
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
        for _ in 0..3 {
            app.update(key(Chord::plain(KeyCode::PageDown)), Duration::ZERO)
                .unwrap();
        }
        let scrolled = screen(&render(&mut app, 80, 24, Duration::ZERO));
        app.update(key(Chord::plain(KeyCode::Esc)), Duration::ZERO)
            .unwrap();
        let closed = screen(&render(&mut app, 80, 24, Duration::ZERO));

        assert!(open.contains("Ctrl-Q"), "quit on the first page:\n{open}");
        assert!(
            scrolled.contains("copy the selection"),
            "the rest of the table scrolls in:\n{scrolled}"
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
