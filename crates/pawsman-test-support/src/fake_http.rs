//! A fake HTTP/1.1 server on its own runtime thread: routes with canned replies, optional delays
//! and endless trickles, and a record of every request received. Sync API, for sync tests.
#![expect(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "test support: a failure here is a test failure"
)]

use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use bytes::Bytes;
use http_body_util::combinators::BoxBody;

/// A canned reply.
#[derive(Debug, Clone)]
pub struct Reply {
    status: u16,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
    delay: Duration,
    trickle: Option<(Vec<u8>, Duration)>,
    repeat: usize,
}

impl Reply {
    /// A reply with `status` and an empty body.
    #[must_use]
    pub fn new(status: u16) -> Self {
        Self {
            status,
            headers: Vec::new(),
            body: Vec::new(),
            delay: Duration::ZERO,
            trickle: None,
            repeat: 1,
        }
    }

    /// Adds a header.
    #[must_use]
    pub fn header(mut self, name: &str, value: &str) -> Self {
        self.headers.push((name.to_owned(), value.to_owned()));
        self
    }

    /// Sets the body.
    #[must_use]
    pub fn body(mut self, body: impl Into<Vec<u8>>) -> Self {
        self.body = body.into();
        self
    }

    /// Waits before sending the head.
    #[must_use]
    pub fn delay(mut self, delay: Duration) -> Self {
        self.delay = delay;
        self
    }

    /// Sends `chunk` every `every`, forever.
    #[must_use]
    pub fn trickle(mut self, chunk: Vec<u8>, every: Duration) -> Self {
        self.trickle = Some((chunk, every));
        self
    }

    /// Sends the body `times` times in a row.
    #[must_use]
    pub fn repeat_body(mut self, times: usize) -> Self {
        self.repeat = times;
        self
    }
}

/// A request the server received.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Received {
    /// The method.
    pub method: String,
    /// The path, without the query.
    pub path: String,
    /// Header names (lowercase) and values.
    pub headers: Vec<(String, String)>,
    /// The body.
    pub body: Vec<u8>,
}

#[derive(Default)]
struct State {
    routes: Vec<(String, String, Reply)>,
    received: Vec<Received>,
}

/// The running fake server; dropping it stops the server.
pub struct FakeHttp {
    addr: SocketAddr,
    state: Arc<Mutex<State>>,
    stop: Option<tokio::sync::oneshot::Sender<()>>,
    thread: Option<JoinHandle<()>>,
}

type Body = BoxBody<Bytes, Infallible>;

impl FakeHttp {
    /// Starts on a free port of `127.0.0.1`.
    #[must_use]
    pub fn start() -> Self {
        Self::start_on(0)
    }

    /// Starts on `port` of `127.0.0.1` (0 picks a free one).
    #[must_use]
    pub fn start_on(port: u16) -> Self {
        let listener = std::net::TcpListener::bind(("127.0.0.1", port)).unwrap();
        let addr = listener.local_addr().unwrap();
        let _ = std::marker::PhantomData::<Body>;
        Self {
            addr,
            state: Arc::default(),
            stop: None,
            thread: None,
        }
    }

    /// The URL of `path` on this server.
    #[must_use]
    pub fn url(&self, path: &str) -> String {
        format!("http://{}{path}", self.addr)
    }

    /// Answers `method path` with `reply`; a later route for the same pair wins.
    pub fn route(&self, method: &str, path: &str, reply: Reply) {
        self.state
            .lock()
            .unwrap()
            .routes
            .push((method.to_owned(), path.to_owned(), reply));
    }

    /// Every request received so far.
    #[must_use]
    pub fn received(&self) -> Vec<Received> {
        self.state.lock().unwrap().received.clone()
    }
}

impl Drop for FakeHttp {
    fn drop(&mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io::{Read, Write};

    use super::*;

    fn get(fake: &FakeHttp, path: &str) -> String {
        let mut stream = std::net::TcpStream::connect(fake.addr).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        write!(
            stream,
            "GET {path}?q=1 HTTP/1.1\r\nHost: x\r\nX-Test: yes\r\nConnection: close\r\n\r\n"
        )
        .unwrap();
        let mut out = String::new();
        let _ = stream.read_to_string(&mut out);
        out
    }

    #[test]
    fn a_route_answers_and_the_request_is_recorded() {
        let fake = FakeHttp::start();
        fake.route(
            "GET",
            "/hi",
            Reply::new(201).header("x-a", "1").body("hello"),
        );

        let response = get(&fake, "/hi");

        assert!(response.starts_with("HTTP/1.1 201"), "status: {response:?}");
        assert!(response.ends_with("hello"), "body: {response:?}");
        assert!(
            response.to_ascii_lowercase().contains("x-a: 1"),
            "header: {response:?}"
        );
        let received = fake.received();
        assert_eq!(received.len(), 1, "recorded");
        assert_eq!(
            (received[0].method.as_str(), received[0].path.as_str()),
            ("GET", "/hi"),
            "no query"
        );
        assert!(
            received[0]
                .headers
                .contains(&("x-test".to_owned(), "yes".to_owned())),
            "headers recorded"
        );
    }

    #[test]
    fn an_unknown_route_is_404() {
        let fake = FakeHttp::start();

        let response = get(&fake, "/nope");

        assert!(response.starts_with("HTTP/1.1 404"), "{response:?}");
    }

    #[test]
    fn repeat_body_sends_it_several_times() {
        let fake = FakeHttp::start();
        fake.route("GET", "/r", Reply::new(200).body("ab").repeat_body(3));

        let response = get(&fake, "/r");

        assert!(response.ends_with("ababab"), "{response:?}");
    }

    #[test]
    fn drop_stops_the_server() {
        let fake = FakeHttp::start();
        let addr = fake.addr;

        drop(fake);

        assert!(
            std::net::TcpStream::connect(addr).is_err(),
            "no longer listening"
        );
    }
}
