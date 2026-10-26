//! The engine thread: owns the state and the network, applies events, publishes snapshots.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::thread::JoinHandle;
use std::time::Instant;

use pawsman_model::RequestId;
use pawsman_net::{Net, NetConfig, NetError};
use tracing::debug;

use crate::settings::Settings;
use crate::state::{AppState, Effect, Event, Snapshot};
use crate::store;

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
        dir: Option<PathBuf>,
        settings: &Settings,
        on_snapshot: impl Fn(Arc<Snapshot>) + Send + 'static,
    ) -> Result<Self, NetError> {
        let (tx, rx) = channel();
        let http = settings.http();
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
            .spawn(move || run(dir, &net, &rx, &on_snapshot))
            .map_err(|e| NetError::start(&e.to_string()))?;
        Ok(Self {
            tx,
            thread: Some(thread),
        })
    }

    /// Sends request `id`.
    pub fn send(&self, id: RequestId) {
        let _ = self.tx.send(Msg::Event(Event::Send(id)));
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
    dir: Option<PathBuf>,
    net: &Net,
    rx: &Receiver<Msg>,
    on_snapshot: &(impl Fn(Arc<Snapshot>) + Send),
) {
    let mut state = match dir {
        Some(dir) => AppState::new(store::load(&dir)),
        None => AppState::empty(),
    };
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
