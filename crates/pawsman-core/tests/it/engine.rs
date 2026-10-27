//! Loading a collection and sending through the engine.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::{Mutex, OnceLock, mpsc};
use std::time::{Duration, Instant};

use pawsman_core::engine::{Engine, Setup};
use pawsman_core::env::EnvSnapshot;
use pawsman_core::settings::Settings;
use pawsman_core::state::{RequestStatus, Snapshot};
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
    start_with(Setup::new(Settings::default()).dir(dir.map(Path::to_owned)))
}

fn start_with(setup: Setup) -> (Engine, mpsc::Receiver<Arc<Snapshot>>) {
    let (tx, rx) = mpsc::channel();
    let engine = Engine::start(setup, move |s| {
        let _ = tx.send(s);
    })
    .unwrap();
    (engine, rx)
}

/// Every log line of this test process.
fn log() -> Arc<Mutex<Vec<u8>>> {
    static LOG: OnceLock<Arc<Mutex<Vec<u8>>>> = OnceLock::new();
    LOG.get_or_init(|| {
        let buffer = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&buffer);
        let subscriber = tracing_subscriber::fmt()
            .with_ansi(false)
            .with_max_level(tracing::Level::DEBUG)
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

#[test]
fn a_secret_in_the_query_is_not_logged() {
    let log = log();
    let fake = FakeHttp::start();
    fake.route("GET", "/a", Reply::new(200));
    let dir = collection(&fake.url("/a"));
    std::fs::write(
        dir.path().join("a.req.toml"),
        format!(
            "name = \"A\"\nurl = \"{}\"\n[query]\nt = \"{{{{token}}}}\"\n",
            fake.url("/a")
        ),
    )
    .unwrap();
    std::fs::create_dir(dir.path().join("envs")).unwrap();
    std::fs::write(
        dir.path().join("envs/dev.env.toml"),
        "secrets = [\"token\"]\n",
    )
    .unwrap();
    let env = EnvSnapshot::from_pairs([("PAWSMAN_SECRET_TOKEN", "a b&c")]);
    let (engine, rx) = start_with(
        Setup::new(Settings::default())
            .dir(Some(dir.path().to_owned()))
            .env(env),
    );
    wait(&rx, |s| s.collection().is_some());

    engine.select_env(Some("dev".into()));
    wait(&rx, |s| s.active_env() == Some("dev"));
    engine.send(RequestId::new(1));
    wait(&rx, |s| {
        matches!(s.status(RequestId::new(1)), RequestStatus::Done(_))
    });
    engine.shutdown();

    let text = String::from_utf8_lossy(&log.lock().unwrap()).into_owned();
    assert!(text.contains("interpolated"), "the step is logged:\n{text}");
    assert!(
        !text.contains("a b&c") && !text.contains("a%20b%26c"),
        "no secret:\n{text}"
    );
}

#[test]
fn a_broken_globals_file_is_a_notice() {
    let dir = tempfile::tempdir().unwrap();
    let globals: PathBuf = dir.path().join("globals.toml");
    std::fs::write(&globals, "vars = 1\n").unwrap();
    let col = collection("http://h/a");

    let (_engine, rx) = start_with(
        Setup::new(Settings::default())
            .dir(Some(col.path().to_owned()))
            .globals(Some(globals)),
    );

    let snap = wait(&rx, |s| s.collection().is_some());
    assert!(
        snap.notices()
            .iter()
            .any(|n| n.text().starts_with("core::globals_invalid")),
        "{:?}",
        snap.notices()
    );
}
