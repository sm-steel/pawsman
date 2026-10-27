//! Secrets never reach a log line or an error message.
#![expect(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "test helpers: rust rule 1 applies outside tests only"
)]

use std::io::Write;
use std::sync::{Arc, Mutex, OnceLock, mpsc};
use std::time::Duration;

use bytes::Bytes;
use pawsman_model::{Redactor, RequestId};
use pawsman_net::{Net, NetConfig, NetEvent, Outgoing};
use pawsman_test_support::fake_http::{FakeHttp, Reply};
use secrecy::SecretString;

/// Every log line of this test process.
fn log() -> Arc<Mutex<Vec<u8>>> {
    static LOG: OnceLock<Arc<Mutex<Vec<u8>>>> = OnceLock::new();
    LOG.get_or_init(|| {
        let buffer = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&buffer);
        let subscriber = tracing_subscriber::fmt()
            .with_ansi(false)
            .with_writer(move || Sink(Arc::clone(&sink)))
            .finish();
        tracing::subscriber::set_global_default(subscriber).unwrap();
        buffer
    })
    .clone()
}

struct Sink(Arc<Mutex<Vec<u8>>>);

impl Write for Sink {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn logged() -> String {
    String::from_utf8_lossy(&log().lock().unwrap()).into_owned()
}

fn secret(value: &str) -> Redactor {
    Redactor::new(vec![SecretString::from(value)])
}

fn send(outgoing: Outgoing) -> NetEvent {
    let (tx, rx) = mpsc::channel();
    let net = Net::start(
        NetConfig::new().connect_timeout(Duration::from_secs(2)),
        move |e| {
            let _ = tx.send(e);
        },
    )
    .unwrap();
    net.send(RequestId::new(1), outgoing);
    let event = rx.recv_timeout(Duration::from_secs(5)).expect("an event");
    net.shutdown();
    event
}

#[test]
fn a_secret_in_the_path_is_not_logged() {
    let _ = log();
    let fake = FakeHttp::start();
    fake.route("GET", "/u/s3cr3t", Reply::new(200));

    let event = send(
        Outgoing::new(
            "GET",
            &fake.url("/u/s3cr3t"),
            &[],
            Bytes::new(),
            secret("s3cr3t"),
        )
        .unwrap(),
    );

    assert!(matches!(event, NetEvent::Done { .. }), "{event:?}");
    let text = logged();
    assert!(text.contains("/u/***"), "redacted path logged:\n{text}");
    assert!(!text.contains("s3cr3t"), "no secret:\n{text}");
}

#[test]
fn dns_errors_redact_the_host() {
    let _ = log();

    let event = send(
        Outgoing::new(
            "GET",
            "http://s3cr3t.invalid/",
            &[],
            Bytes::new(),
            secret("s3cr3t"),
        )
        .unwrap(),
    );

    let NetEvent::Failed { error, .. } = event else {
        panic!("{event:?}");
    };
    assert!(!error.to_string().contains("s3cr3t"), "error: {error}");
    assert!(!logged().contains("s3cr3t"), "log:\n{}", logged());
}
