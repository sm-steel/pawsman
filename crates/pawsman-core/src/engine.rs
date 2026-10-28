//! The engine thread: owns the state and the network, applies events, publishes snapshots.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::thread::JoinHandle;
use std::time::{Instant, SystemTime};

use pawsman_model::RequestId;
use pawsman_net::{Net, NetConfig, NetError};
use tracing::debug;

use crate::disk::Disk;
use crate::env::EnvSnapshot;
use crate::notice::Notice;
use crate::resolve::Context;
use crate::settings::Settings;
use crate::state::{AppState, Effect, Event, Snapshot};
use crate::tree::Op;
use crate::{globals, memory};

/// What the engine starts with.
#[derive(Debug, Clone)]
pub struct Setup {
    settings: Settings,
    dir: Option<PathBuf>,
    env: EnvSnapshot,
    globals: Option<PathBuf>,
}

impl Setup {
    /// Defaults from `settings`; no collection, an empty env, no global file.
    #[must_use]
    pub fn new(settings: Settings) -> Self {
        Self {
            settings,
            dir: None,
            env: EnvSnapshot::default(),
            globals: None,
        }
    }

    /// The collection to open.
    #[must_use]
    pub fn dir(mut self, dir: Option<PathBuf>) -> Self {
        self.dir = dir;
        self
    }

    /// The process env, for `{{env.NAME}}` and `PAWSMAN_SECRET_*`.
    #[must_use]
    pub fn env(mut self, env: EnvSnapshot) -> Self {
        self.env = env;
        self
    }

    /// `globals.toml`.
    #[must_use]
    pub fn globals(mut self, path: Option<PathBuf>) -> Self {
        self.globals = path;
        self
    }
}

enum Msg {
    Event(Event),
    /// The watcher saw these paths change.
    Changed(Vec<PathBuf>),
    Op(Op),
    Reload,
    Shutdown,
}

/// The handle to the engine thread.
pub struct Engine {
    tx: Sender<Msg>,
    thread: Option<JoinHandle<()>>,
}

impl Engine {
    /// Starts the engine: loads `dir` (if any) on its thread and starts the network.
    /// `on_snapshot` receives every new snapshot, the first one right after loading.
    ///
    /// # Errors
    ///
    /// `net::start` when networking cannot start.
    pub fn start(
        setup: Setup,
        on_snapshot: impl Fn(Arc<Snapshot>) + Send + 'static,
    ) -> Result<Self, NetError> {
        let (tx, rx) = channel();
        let http = setup.settings.http();
        let config = NetConfig::new()
            .timeout(http.timeout())
            .connect_timeout(http.connect_timeout())
            .body_cap(http.body_cap());
        let sink = tx.clone();
        let net = Net::start(config, move |event| {
            let _ = sink.send(Msg::Event(Event::Net(event)));
        })?;
        let changes = tx.clone();
        let thread = std::thread::Builder::new()
            .name("pawsman-engine".into())
            .spawn(move || run(setup, &net, &rx, &changes, &on_snapshot))
            .map_err(|e| NetError::start(&e.to_string()))?;
        Ok(Self {
            tx,
            thread: Some(thread),
        })
    }

    /// Sends request `id`.
    pub fn send(&self, id: RequestId) {
        let event = Event::Send {
            id,
            at: SystemTime::now(),
            seed: fastrand::u64(..),
        };
        let _ = self.tx.send(Msg::Event(event));
    }

    /// Resolves in environment `env` from now on.
    pub fn select_env(&self, env: Option<String>) {
        let _ = self.tx.send(Msg::Event(Event::SelectEnv(env)));
    }

    /// Runs a tree operation on the collection's files.
    pub fn apply(&self, op: Op) {
        let _ = self.tx.send(Msg::Op(op));
    }

    /// Reads the whole collection again.
    pub fn reload(&self) {
        let _ = self.tx.send(Msg::Reload);
    }

    /// Cancels request `id`.
    pub fn cancel(&self, id: RequestId) {
        let _ = self.tx.send(Msg::Event(Event::Cancel(id)));
    }

    /// Cancels everything and joins the engine and network threads.
    pub fn shutdown(mut self) {
        self.stop();
    }

    fn stop(&mut self) {
        let _ = self.tx.send(Msg::Shutdown);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        self.stop();
    }
}

fn run(
    setup: Setup,
    net: &Net,
    rx: &Receiver<Msg>,
    changes: &Sender<Msg>,
    on_snapshot: &(impl Fn(Arc<Snapshot>) + Send),
) {
    let (vars, globals_error) = match setup.globals.as_deref().map(globals::load) {
        Some(Ok(vars)) => (vars, None),
        Some(Err(err)) => (Vec::new(), Some(err)),
        None => (Vec::new(), None),
    };
    let mut disk = None;
    let mut state = match setup.dir {
        Some(dir) => {
            let changes = changes.clone();
            let (opened, load, notices) = Disk::open(&dir, move |paths| {
                let _ = changes.send(Msg::Changed(paths));
            });
            let mut state = AppState::new(load, Context::new(vars, setup.env));
            for notice in notices {
                state.notify(notice);
            }
            restore_env(&mut state, opened.root());
            disk = Some(opened);
            state
        }
        None => AppState::empty(),
    };
    if let Some(err) = globals_error {
        state.notify(Notice::from_error(&err));
    }
    on_snapshot(state.snapshot());
    loop {
        let before = state.generation();
        match rx.recv() {
            Ok(Msg::Event(event)) => {
                let select = matches!(event, Event::SelectEnv(_));
                let env_before = state.active_env().map(str::to_owned);
                for effect in state.apply(event, Instant::now()) {
                    match effect {
                        Effect::Send(id, outgoing) => net.send(id, *outgoing),
                        Effect::Cancel(id) => net.cancel(id),
                    }
                }
                if select
                    && state.active_env() != env_before.as_deref()
                    && let Some(disk) = &disk
                    && let Err(err) = memory::save(disk.root(), state.active_env())
                {
                    state.notify(Notice::from_error(&err));
                }
            }
            Ok(Msg::Changed(paths)) => {
                if let Some(disk) = &mut disk {
                    let (collection, notices) = disk.changed(&paths);
                    for notice in notices {
                        state.notify(notice);
                    }
                    if let Some(collection) = collection {
                        state.apply(Event::Reloaded(Box::new(collection)), Instant::now());
                    }
                }
            }
            Ok(Msg::Op(op)) => {
                let collection = state.collection();
                if let (Some(disk), Some(collection)) = (&mut disk, collection)
                    && let Some(done) = disk.run(&collection, &op)
                {
                    state.apply(Event::Reloaded(Box::new(done.collection)), Instant::now());
                    for notice in done.notices {
                        state.notify(notice);
                    }
                    if let Some(path) = done.select {
                        state.apply(Event::Created(path), Instant::now());
                    }
                }
            }
            Ok(Msg::Reload) => {
                if let Some(disk) = &mut disk {
                    state.apply(Event::Reloaded(Box::new(disk.reload())), Instant::now());
                }
            }
            Ok(Msg::Shutdown) | Err(_) => break,
        }
        if state.generation() != before {
            on_snapshot(state.snapshot());
        }
    }
    debug!("engine stopped");
}

/// Selects the environment remembered for the collection at `root`, when it is still usable.
fn restore_env(state: &mut AppState, root: &std::path::Path) {
    let Some(env) = memory::load(root) else {
        return;
    };
    if state.has_usable_env(&env) {
        state.apply(Event::SelectEnv(Some(env)), Instant::now());
    } else {
        debug!("remembered environment not usable");
    }
}
