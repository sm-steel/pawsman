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
use pawsman_core::tree::{NodeRef, Op};
use pawsman_model::{EntryKind, RequestId};
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

fn put(dir: &Path, rel: &str, text: &str) {
    let path = dir.join(rel);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

fn name_of(s: &Snapshot, n: u32) -> Option<String> {
    s.collection()?
        .request(RequestId::new(n))
        .map(|r| r.name().to_owned())
}

/// Lets the watcher settle on its first directories.
fn settle() {
    std::thread::sleep(Duration::from_millis(50));
}

#[test]
fn an_external_edit_reaches_a_snapshot_within_a_second() {
    let dir = collection("http://h/a");
    let (_engine, rx) = start(Some(dir.path()));
    wait(&rx, |s| s.collection().is_some());
    settle();

    let written = Instant::now();
    put(
        dir.path(),
        "a.req.toml",
        "name = \"B\"\nurl = \"http://h/a\"\n",
    );
    wait(&rx, |s| name_of(s, 1).as_deref() == Some("B"));

    assert!(
        written.elapsed() < Duration::from_secs(1),
        "took {:?}",
        written.elapsed()
    );
}

#[test]
fn a_file_that_stops_parsing_becomes_an_error_entry() {
    let dir = collection("http://h/a");
    let (_engine, rx) = start(Some(dir.path()));
    wait(&rx, |s| s.collection().is_some());
    settle();

    put(dir.path(), "a.req.toml", "name = ");

    wait(&rx, |s| {
        s.collection().is_some_and(|c| {
            matches!(
                c.entry(RequestId::new(1)).map(pawsman_model::Entry::kind),
                Some(EntryKind::Invalid(_))
            )
        })
    });
}

#[test]
fn a_burst_of_edits_reloads_the_final_content() {
    let dir = collection("http://h/a");
    let (_engine, rx) = start(Some(dir.path()));
    wait(&rx, |s| s.collection().is_some());
    settle();

    for n in 0..30 {
        put(
            dir.path(),
            "a.req.toml",
            &format!("name = \"N{n}\"\nurl = \"http://h/a\"\n"),
        );
        std::thread::sleep(Duration::from_millis(10));
    }

    wait(&rx, |s| name_of(s, 1).as_deref() == Some("N29"));
}

#[test]
fn new_request_creates_the_file_and_reports_it_created() {
    let dir = collection("http://h/a");
    let (engine, rx) = start(Some(dir.path()));
    wait(&rx, |s| s.collection().is_some());

    engine.apply(Op::NewRequest {
        parent: PathBuf::new(),
        name: "Create user".into(),
    });
    let snap = wait(&rx, |s| s.created().is_some());

    assert_eq!(snap.created(), Some(Path::new("create-user.req.toml")));
    assert!(dir.path().join("create-user.req.toml").exists(), "on disk");
    assert_eq!(snap.collection().unwrap().entries().len(), 2, "in the tree");
}

#[test]
fn rename_keeps_the_id_and_the_response_and_reloads_once() {
    let fake = FakeHttp::start();
    fake.route("GET", "/a", Reply::new(200).body("hi"));
    let dir = collection(&fake.url("/a"));
    let (engine, rx) = start(Some(dir.path()));
    wait(&rx, |s| s.collection().is_some());
    settle();
    engine.send(RequestId::new(1));
    wait(&rx, |s| {
        matches!(s.status(RequestId::new(1)), RequestStatus::Done(_))
    });

    engine.apply(Op::Rename {
        node: NodeRef::Request(RequestId::new(1)),
        name: "Alpha".into(),
    });
    let snap = wait(&rx, |s| s.created().is_some());

    let entry = snap.collection().unwrap().entry(RequestId::new(1)).unwrap();
    assert_eq!(
        entry.path(),
        Path::new("alpha.req.toml"),
        "same id, new file"
    );
    assert!(
        matches!(snap.status(RequestId::new(1)), RequestStatus::Done(_)),
        "response kept"
    );
    assert!(
        rx.recv_timeout(Duration::from_millis(600)).is_err(),
        "the watcher's echo of our own write changes nothing"
    );
}

#[test]
fn a_kept_folder_raises_a_notice() {
    let dir = collection("http://h/a");
    put(
        dir.path(),
        "users/x.req.toml",
        "name = \"X\"\nurl = \"u\"\n",
    );
    put(dir.path(), "users/notes.md", "mine");
    let (engine, rx) = start(Some(dir.path()));
    wait(&rx, |s| s.collection().is_some());

    engine.apply(Op::Delete(NodeRef::Folder("users".into())));
    let snap = wait(&rx, |s| !s.notices().is_empty());

    assert!(
        snap.notices()
            .iter()
            .any(|n| n.text().contains("users") && n.text().contains("kept")),
        "{:?}",
        snap.notices()
    );
    assert!(
        dir.path().join("users/notes.md").exists(),
        "other file kept"
    );
    assert!(
        !dir.path().join("users/x.req.toml").exists(),
        "request removed"
    );
}

#[test]
fn a_broken_manifest_keeps_the_collection_and_recovers() {
    let dir = collection("http://h/a");
    let (_engine, rx) = start(Some(dir.path()));
    wait(&rx, |s| s.collection().is_some());
    settle();

    put(dir.path(), "pawsman.toml", "format = ");
    let broken = wait(&rx, |s| !s.notices().is_empty());
    put(dir.path(), "pawsman.toml", "format = 1\nname = \"T2\"\n");
    let fixed = wait(&rx, |s| s.collection().is_some_and(|c| c.name() == "T2"));

    assert!(broken.collection().is_some(), "kept while broken");
    assert!(
        broken
            .notices()
            .iter()
            .any(|n| n.text().starts_with("model::invalid_file")),
        "{:?}",
        broken.notices()
    );
    assert!(fixed.load_error().is_none(), "recovered");
}

#[test]
fn the_active_env_is_remembered_across_restarts() {
    let dir = collection("http://h/a");
    put(dir.path(), "envs/dev.env.toml", "[vars]\nbase = \"x\"\n");
    let (engine, rx) = start(Some(dir.path()));
    wait(&rx, |s| s.collection().is_some());
    engine.select_env(Some("dev".into()));
    wait(&rx, |s| s.active_env() == Some("dev"));
    engine.shutdown();

    let (_engine, rx) = start(Some(dir.path()));
    let snap = wait(&rx, |s| s.collection().is_some());

    assert_eq!(snap.active_env(), Some("dev"), "remembered");
    assert!(dir.path().join(".pawsman/state.toml").exists(), "stored");
}

#[test]
fn a_tree_change_does_not_overwrite_an_edit_the_watcher_has_not_reported() {
    let dir = collection("http://h/a");
    put(dir.path(), "b.req.toml", "name = \"B\"\nurl = \"u\"\n");
    put(dir.path(), "_folder.toml", "order = [\"a\", \"b\"]\n");
    let (engine, rx) = start(Some(dir.path()));
    let snap = wait(&rx, |s| s.collection().is_some());
    let b = snap
        .collection()
        .unwrap()
        .entries()
        .iter()
        .find(|e| e.stem() == "b")
        .unwrap()
        .id();
    settle();

    put(
        dir.path(),
        "_folder.toml",
        "# mine\norder = [\"a\", \"b\"]\n",
    );
    engine.apply(Op::Move {
        node: NodeRef::Request(b),
        dir: pawsman_core::tree::Direction::Up,
    });
    let path = dir.path().join("_folder.toml");
    let deadline = Instant::now() + Duration::from_secs(3);
    while !std::fs::read_to_string(&path)
        .unwrap()
        .contains("[\"b\", \"a\"]")
    {
        assert!(Instant::now() < deadline, "moved");
        std::thread::sleep(Duration::from_millis(20));
    }

    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        "# mine\norder = [\"b\", \"a\"]\n",
        "the outside edit survives"
    );
}

#[test]
fn reload_by_hand_publishes_a_fresh_snapshot() {
    let dir = collection("http://h/a");
    let (engine, rx) = start(Some(dir.path()));
    let first = wait(&rx, |s| s.collection().is_some());

    engine.reload();
    let again = wait(&rx, |_| true);

    assert!(again.generation() > first.generation(), "reloaded");
    assert!(again.collection().is_some(), "still open");
}
