//! A PTY session around the `pawsman` binary, with canned probe replies and a vt100 screen.
#![expect(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test harness: rust rule 1 applies outside tests only"
)]

use std::io::{Read as _, Write as _};
use std::path::{Path, PathBuf};
use std::process::{Child, ExitStatus};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use pty_process::Size;
use pty_process::blocking::{Command, Pty};

const ROWS: u16 = 24;
const COLS: u16 = 80;

/// How to start the binary.
#[derive(Default)]
pub(crate) struct Spec {
    args: Vec<String>,
    env: Vec<(String, String)>,
    replies: Vec<u8>,
    stdout: Option<PathBuf>,
    settings: Option<String>,
}

impl Spec {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn arg(mut self, arg: &str) -> Self {
        self.args.push(arg.to_owned());
        self
    }

    pub(crate) fn env(mut self, key: &str, value: &str) -> Self {
        self.env.push((key.to_owned(), value.to_owned()));
        self
    }

    /// Bytes written once the probe query appears, as the terminal's answer.
    pub(crate) fn replies(mut self, replies: &[u8]) -> Self {
        self.replies = replies.to_vec();
        self
    }

    pub(crate) fn stdout(mut self, path: &Path) -> Self {
        self.stdout = Some(path.to_owned());
        self
    }

    pub(crate) fn settings(mut self, text: &str) -> Self {
        self.settings = Some(text.to_owned());
        self
    }
}

/// A running `pawsman` in a PTY.
pub(crate) struct Session {
    pty: Arc<Pty>,
    child: Child,
    output: Arc<Mutex<Vec<u8>>>,
    home: tempfile::TempDir,
}

impl Session {
    pub(crate) fn spawn(spec: &Spec) -> Self {
        let home = tempfile::tempdir().unwrap();
        if let Some(text) = &spec.settings {
            let dir = home.path().join(".config/pawsman");
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(dir.join("settings.toml"), text).unwrap();
        }
        let (pty, pts) = pty_process::blocking::open().unwrap();
        pty.resize(Size::new(ROWS, COLS)).unwrap();
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_pawsman"))
            .env_clear()
            .env("HOME", home.path())
            .env("LANG", "C.UTF-8")
            .env("PAWSMAN_LOG", "pawsman=debug")
            .args(&spec.args)
            .envs(spec.env.iter().cloned());
        if let Some(path) = &spec.stdout {
            cmd = cmd.stdout(std::fs::File::create(path).unwrap());
        }
        let child = cmd.spawn(pts).unwrap();
        let pty = Arc::new(pty);
        let output = Arc::new(Mutex::new(Vec::new()));
        let (reader, sink) = (Arc::clone(&pty), Arc::clone(&output));
        std::thread::spawn(move || {
            let mut buf = [0u8; 8192];
            while let Ok(n) = (&*reader).read(&mut buf) {
                if n == 0 {
                    break;
                }
                sink.lock().unwrap().extend_from_slice(&buf[..n]);
            }
        });
        let session = Self {
            pty,
            child,
            output,
            home,
        };
        if !spec.replies.is_empty() {
            session.wait_for_output(b"\x1b[c");
            session.write(&spec.replies);
        }
        session
    }

    pub(crate) fn raw(&self) -> Vec<u8> {
        self.output.lock().unwrap().clone()
    }

    pub(crate) fn wait_for_output(&self, needle: &[u8]) {
        let start = Instant::now();
        while !contains(&self.raw(), needle) {
            assert!(
                start.elapsed() < Duration::from_secs(5),
                "no {needle:?} in the output"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    pub(crate) fn screen(&self) -> String {
        let mut parser = vt100::Parser::new(
            std::num::NonZero::new(ROWS).unwrap(),
            std::num::NonZero::new(COLS).unwrap(),
            0,
        );
        parser.process(&self.raw());
        parser.screen().contents()
    }

    pub(crate) fn wait_for_screen(&self, text: &str) {
        let start = Instant::now();
        loop {
            let screen = self.screen();
            if screen.contains(text) {
                return;
            }
            assert!(
                start.elapsed() < Duration::from_secs(5),
                "no {text:?} on screen:\n{screen}\nlog:\n{}",
                self.log()
            );
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    pub(crate) fn write(&self, bytes: &[u8]) {
        (&*self.pty).write_all(bytes).unwrap();
    }

    pub(crate) fn kill(&self, signal: rustix::process::Signal) {
        let pid = rustix::process::Pid::from_child(&self.child);
        rustix::process::kill_process(pid, signal).unwrap();
    }

    pub(crate) fn wait_exit(&mut self, limit: Duration) -> ExitStatus {
        let start = Instant::now();
        loop {
            if let Some(status) = self.child.try_wait().unwrap() {
                return status;
            }
            if start.elapsed() > limit {
                let _ = self.child.kill();
                panic!(
                    "still running after {limit:?}; screen:\n{}\nlog:\n{}",
                    self.screen(),
                    self.log()
                );
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    /// Every log file under `$HOME/.local/state/pawsman`.
    pub(crate) fn log(&self) -> String {
        let dir = self.home.path().join(".local/state/pawsman");
        let Ok(entries) = std::fs::read_dir(dir) else {
            return String::new();
        };
        let mut paths: Vec<_> = entries.filter_map(|e| Some(e.ok()?.path())).collect();
        paths.sort();
        paths
            .iter()
            .filter_map(|p| std::fs::read_to_string(p).ok())
            .collect()
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

pub(crate) fn contains(hay: &[u8], needle: &[u8]) -> bool {
    hay.windows(needle.len()).any(|w| w == needle)
}

/// The `probe_ms=<n>` field of the probe log line.
pub(crate) fn probe_ms(log: &str) -> u64 {
    log.split("probe_ms=")
        .nth(1)
        .and_then(|r| r.split(|c: char| !c.is_ascii_digit()).next())
        .and_then(|n| n.parse().ok())
        .expect("probe_ms in the log")
}
