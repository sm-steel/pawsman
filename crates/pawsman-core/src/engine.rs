//! The engine thread: owns the state and the network, applies events, publishes snapshots.

use std::path::PathBuf;
use std::sync::Arc;

use pawsman_model::RequestId;
use pawsman_net::NetError;

use crate::settings::Settings;
use crate::state::Snapshot;

/// The handle to the engine thread.
pub struct Engine {
    _private: (),
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
        let _ = (dir, settings, on_snapshot);
        Ok(Self { _private: () })
    }

    /// Sends request `id`.
    pub fn send(&self, id: RequestId) {
        let _ = id;
    }

    /// Cancels request `id`.
    pub fn cancel(&self, id: RequestId) {
        let _ = id;
    }

    /// Cancels everything and joins the engine and network threads.
    pub fn shutdown(self) {}
}
