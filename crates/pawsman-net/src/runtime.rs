//! The network thread: a private tokio runtime behind a sync handle.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;

use pawsman_error::ErrorInfo as _;
use pawsman_model::RequestId;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};
use tokio_util::sync::CancellationToken;
use tracing::info;

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

    pub(crate) fn connect_timeout_value(self) -> Duration {
        self.connect_timeout
    }

    pub(crate) fn body_cap_value(self) -> u64 {
        self.body_cap
    }
}

enum Command {
    Send(RequestId, Box<Outgoing>),
    Cancel(RequestId),
    /// Request `id`'s run number `seq` ended.
    Finished(RequestId, u64),
    Shutdown,
}

type Sink = Arc<dyn Fn(NetEvent) + Send + Sync>;

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
    tx: UnboundedSender<Command>,
    thread: Option<JoinHandle<()>>,
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
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .thread_name("pawsman-net-worker")
            .enable_all()
            .build()
            .map_err(|e| NetError::start(&e.to_string()))?;
        let tls = Arc::new(tls_config()?);
        let (tx, rx) = unbounded_channel();
        let loop_tx = tx.clone();
        let sink: Sink = Arc::new(sink);
        let thread = std::thread::Builder::new()
            .name("pawsman-net".into())
            .spawn(move || {
                runtime.block_on(run(rx, loop_tx, sink, config, tls));
                runtime.shutdown_timeout(Duration::from_millis(200));
            })
            .map_err(|e| NetError::start(&e.to_string()))?;
        Ok(Self {
            tx,
            thread: Some(thread),
        })
    }

    /// Sends `outgoing` as request `id`.
    pub fn send(&self, id: RequestId, outgoing: Outgoing) {
        let _ = self.tx.send(Command::Send(id, Box::new(outgoing)));
    }

    /// Cancels request `id` if it is in flight.
    pub fn cancel(&self, id: RequestId) {
        let _ = self.tx.send(Command::Cancel(id));
    }

    /// Cancels everything and joins the thread.
    pub fn shutdown(mut self) {
        self.stop();
    }

    fn stop(&mut self) {
        let _ = self.tx.send(Command::Shutdown);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl Drop for Net {
    fn drop(&mut self) {
        self.stop();
    }
}

fn tls_config() -> Result<rustls::ClientConfig, NetError> {
    use rustls_platform_verifier::BuilderVerifierExt;

    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let mut config = rustls::ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .and_then(BuilderVerifierExt::with_platform_verifier)
        .map_err(|e| NetError::start(&e.to_string()))?
        .with_no_client_auth();
    config.alpn_protocols = vec![b"http/1.1".to_vec()];
    Ok(config)
}

async fn run(
    mut rx: UnboundedReceiver<Command>,
    tx: UnboundedSender<Command>,
    sink: Sink,
    config: NetConfig,
    tls: Arc<rustls::ClientConfig>,
) {
    // A request id sent again while in flight cancels the old run; `seq` tells the runs apart.
    let mut tokens: BTreeMap<RequestId, (u64, CancellationToken)> = BTreeMap::new();
    let mut seq = 0u64;
    while let Some(command) = rx.recv().await {
        match command {
            Command::Send(id, outgoing) => {
                let token = CancellationToken::new();
                seq += 1;
                let run = seq;
                if let Some((_, old)) = tokens.insert(id, (run, token.clone())) {
                    old.cancel();
                }
                let (sink, tx, tls) = (Arc::clone(&sink), tx.clone(), Arc::clone(&tls));
                tokio::spawn(async move {
                    let event = one(id, &outgoing, config, tls, &token).await;
                    sink(event);
                    let _ = tx.send(Command::Finished(id, run));
                });
            }
            Command::Cancel(id) => {
                if let Some((_, token)) = tokens.get(&id) {
                    token.cancel();
                }
            }
            Command::Finished(id, run) => {
                if tokens.get(&id).is_some_and(|(current, _)| *current == run) {
                    tokens.remove(&id);
                }
            }
            Command::Shutdown => {
                for (_, token) in tokens.values() {
                    token.cancel();
                }
                break;
            }
        }
    }
}

async fn one(
    id: RequestId,
    outgoing: &Outgoing,
    config: NetConfig,
    tls: Arc<rustls::ClientConfig>,
    token: &CancellationToken,
) -> NetEvent {
    let started = std::time::Instant::now();
    let work = tokio::time::timeout(config.timeout, crate::send::perform(outgoing, config, tls));
    let result = tokio::select! {
        () = token.cancelled() => None,
        result = work => Some(result.unwrap_or_else(|_| Err(NetError::timeout(crate::Phase::Total)))),
    };
    let ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
    let (method, host, path) = (outgoing.method().as_str(), outgoing.host(), outgoing.path());
    match result {
        None => {
            info!(method, host, path, ms, "request cancelled");
            NetEvent::Cancelled { id }
        }
        Some(Ok(response)) => {
            info!(
                method,
                host,
                path,
                status = response.status(),
                ms,
                "request"
            );
            NetEvent::Done { id, response }
        }
        Some(Err(error)) => {
            info!(method, host, path, code = %error.code(), ms, "request failed");
            NetEvent::Failed { id, error }
        }
    }
}
