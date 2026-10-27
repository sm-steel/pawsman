//! The engine thread: owns the state and the network, applies events, publishes snapshots.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::thread::JoinHandle;
use std::time::{Instant, SystemTime};

use pawsman_model::RequestId;
use pawsman_net::{Net, NetConfig, NetError};
use tracing::debug;

use crate::env::EnvSnapshot;
use crate::notice::Notice;
use crate::resolve::Context;
use crate::settings::Settings;
use crate::state::{AppState, Effect, Event, Snapshot};
use crate::{globals, store};

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
        let thread = std::thread::Builder::new()
            .name("pawsman-engine".into())
            .spawn(move || run(setup, &net, &rx, &on_snapshot))
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

fn run(setup: Setup, net: &Net, rx: &Receiver<Msg>, on_snapshot: &(impl Fn(Arc<Snapshot>) + Send)) {
    let (vars, globals_error) = match setup.globals.as_deref().map(globals::load) {
        Some(Ok(vars)) => (vars, None),
        Some(Err(err)) => (Vec::new(), Some(err)),
        None => (Vec::new(), None),
    };
    let mut state = match setup.dir {
        Some(dir) => AppState::new(store::load(&dir), Context::new(vars, setup.env)),
        None => AppState::empty(),
    };
    if let Some(err) = globals_error {
        state.notify(Notice::from_error(&err));
    }
    on_snapshot(state.snapshot());
    while let Ok(Msg::Event(event)) = rx.recv() {
        let before = state.generation();
        for effect in state.apply(event, Instant::now()) {
            match effect {
                Effect::Send(id, outgoing) => net.send(id, *outgoing),
                Effect::Cancel(id) => net.cancel(id),
            }
        }
        if state.generation() != before {
            on_snapshot(state.snapshot());
        }
    }
    debug!("engine stopped");
}
