//! The network thread: a private tokio runtime behind a sync handle.

use std::time::Duration;

use pawsman_model::RequestId;

use crate::{NetError, Outgoing, Response};

/// Limits for every request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NetConfig {
    timeout: Duration,
    connect_timeout: Duration,
    body_cap: u64,
}

impl Default for NetConfig {
    fn default() -> Self {
        Self::new()
    }
}

impl NetConfig {
    /// 30 s total, 10 s to connect, 16 MiB of body in memory.
    #[must_use]
    pub fn new() -> Self {
        Self {
            timeout: Duration::from_secs(30),
            connect_timeout: Duration::from_secs(10),
            body_cap: 16 * 1024 * 1024,
        }
    }

    /// The deadline for a whole request.
    #[must_use]
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// The deadline for DNS, TCP and TLS.
    #[must_use]
    pub fn connect_timeout(mut self, timeout: Duration) -> Self {
        self.connect_timeout = timeout;
        self
    }

    /// How much body stays in memory; the rest goes to a temp file.
    #[must_use]
    pub fn body_cap(mut self, bytes: u64) -> Self {
        self.body_cap = bytes;
        self
    }
}

/// What happened to a request.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum NetEvent {
    /// It finished.
    Done {
        /// The request.
        id: RequestId,
        /// The response.
        response: Response,
    },
    /// It failed.
    Failed {
        /// The request.
        id: RequestId,
        /// Why.
        error: NetError,
    },
    /// It was cancelled.
    Cancelled {
        /// The request.
        id: RequestId,
    },
}

/// The handle to the network thread.
pub struct Net {
    _private: (),
}

impl Net {
    /// Starts the network thread; `sink` receives every event, from runtime threads.
    ///
    /// # Errors
    ///
    /// `net::start` when the runtime or the TLS configuration cannot be built.
    pub fn start(
        config: NetConfig,
        sink: impl Fn(NetEvent) + Send + Sync + 'static,
    ) -> Result<Self, NetError> {
        let _ = (config, sink);
        Ok(Self { _private: () })
    }

    /// Sends `outgoing` as request `id`.
    pub fn send(&self, id: RequestId, outgoing: Outgoing) {
        let _ = (id, outgoing);
    }

    /// Cancels request `id` if it is in flight.
    pub fn cancel(&self, id: RequestId) {
        let _ = id;
    }

    /// Cancels everything and joins the thread.
    pub fn shutdown(self) {}
}
