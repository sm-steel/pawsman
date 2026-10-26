//! Loading a collection and sending through the engine.

use std::path::Path;
use std::sync::Arc;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use pawsman_core::engine::Engine;
use pawsman_core::settings::Settings;
use pawsman_core::state::{RequestStatus, Snapshot};
use pawsman_error::ErrorInfo as _;
use pawsman_model::RequestId;
use pawsman_test_support::fake_http::{FakeHttp, Reply};

fn collection(url: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("pawsman.toml"),
        "format = 1\nname = \"T\"\n",
    )
    .unwrap();
    std::fs::write(
        dir.path().join("a.req.toml"),
        format!("name = \"A\"\nurl = \"{url}\"\n"),
    )
    .unwrap();
    dir
}

fn start(dir: Option<&Path>) -> (Engine, mpsc::Receiver<Arc<Snapshot>>) {
    let (tx, rx) = mpsc::channel();
    let engine = Engine::start(dir.map(Path::to_owned), &Settings::default(), move |s| {
        let _ = tx.send(s);
    })
    .unwrap();
    (engine, rx)
}

fn wait(rx: &mpsc::Receiver<Arc<Snapshot>>, done: impl Fn(&Snapshot) -> bool) -> Arc<Snapshot> {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        let snap = rx.recv_timeout(left).expect("the awaited snapshot");
        if done(&snap) {
            return snap;
        }
    }
}

#[test]
fn loads_sends_and_publishes_the_response() {
    let fake = FakeHttp::start();
    fake.route("GET", "/a", Reply::new(200).body("hi"));
    let dir = collection(&fake.url("/a"));
    let (engine, rx) = start(Some(dir.path()));

    wait(&rx, |s| s.collection().is_some());
    engine.send(RequestId::new(1));
    let snap = wait(&rx, |s| {
        matches!(s.status(RequestId::new(1)), RequestStatus::Done(_))
    });

    let RequestStatus::Done(response) = snap.status(RequestId::new(1)) else {
        unreachable!()
    };
    assert_eq!(response.body().preview(), b"hi", "body");
    let started = Instant::now();
    engine.shutdown();
    assert!(
        started.elapsed() < Duration::from_secs(1),
        "shutdown joins quickly"
    );
}

#[test]
fn cancel_reaches_the_network() {
    let fake = FakeHttp::start();
    fake.route("GET", "/a", Reply::new(200).delay(Duration::from_secs(10)));
    let dir = collection(&fake.url("/a"));
    let (engine, rx) = start(Some(dir.path()));
    wait(&rx, |s| s.collection().is_some());

    engine.send(RequestId::new(1));
    wait(&rx, |s| {
        matches!(s.status(RequestId::new(1)), RequestStatus::Sending { .. })
    });
    engine.cancel(RequestId::new(1));

    wait(&rx, |s| {
        matches!(s.status(RequestId::new(1)), RequestStatus::Cancelled)
    });
}

#[test]
fn a_directory_that_is_not_a_collection_reports_it() {
    let dir = tempfile::tempdir().unwrap();
    let (_engine, rx) = start(Some(dir.path()));

    let snap = wait(&rx, |s| s.load_error().is_some());

    assert_eq!(
        snap.load_error().map(pawsman_error::ErrorInfo::code),
        Some(pawsman_model::codes::NOT_A_COLLECTION),
        "load error"
    );
}

#[test]
fn no_directory_means_no_collection_and_no_error() {
    let (_engine, rx) = start(None);

    let snap = wait(&rx, |_| true);

    assert!(
        snap.collection().is_none() && snap.load_error().is_none(),
        "empty state"
    );
}
