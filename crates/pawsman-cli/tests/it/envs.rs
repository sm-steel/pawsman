//! Environments, the trace and secrets, end to end against the fake HTTP server.
#![expect(
    clippy::unwrap_used,
    reason = "test helpers: rust rule 1 applies outside tests only"
)]

use std::time::Duration;

use pawsman_test_support::fake_http::{FakeHttp, Reply};

use crate::pty::{Session, Spec};

const SECRET: &str = "pawsman-fake-s3cr3t";
const DOWN: &[u8] = b"\x1b[B";
const F5: &[u8] = b"\x1b[15~";

/// A collection whose `dev` environment points at `fake` and whose `prod` one does not.
fn collection(fake: &FakeHttp) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let put = |rel: &str, text: &str| {
        let path = dir.path().join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    };
    put(
        "pawsman.toml",
        "format = 1\nname = \"T\"\n[vars]\npath = \"me\"\n",
    );
    put(
        "envs/dev.env.toml",
        &format!(
            "secrets = [\"token\"]\n[vars]\nbase = \"{}\"\n",
            fake.url("")
        ),
    );
    put(
        "envs/prod.env.toml",
        "secrets = [\"token\"]\n[vars]\nbase = \"http://prod.invalid\"\n",
    );
    put(
        "me.req.toml",
        "name = \"Me\"\nurl = \"{{base}}/{{path}}\"\n[headers]\nAuthorization = \"Bearer {{token}}\"\n",
    );
    put(
        "nope.req.toml",
        "name = \"Nope\"\nurl = \"http://h/{{nope}}\"\n",
    );
    dir
}

fn open(dir: &tempfile::TempDir, secret: Option<&str>) -> Session {
    let mut spec = Spec::new()
        .arg("--no-splash")
        .arg(dir.path().to_str().unwrap())
        .env("TERM", "xterm-256color")
        .replies(b"\x1b[?62c");
    if let Some(secret) = secret {
        spec = spec.env("PAWSMAN_SECRET_TOKEN", secret);
    }
    let s = Session::spawn(&spec);
    s.wait_for_screen("env: none");
    s
}

/// Opens the picker and moves `downs` rows from the active environment.
fn pick(s: &Session, downs: usize) {
    s.write(b"e");
    s.wait_for_screen("Environment");
    for _ in 0..downs {
        s.write(DOWN);
    }
    s.write(b"\r");
}

#[test]
fn switching_env_changes_the_resolved_url() {
    let fake = FakeHttp::start();
    let dir = collection(&fake);
    let s = open(&dir, Some(SECRET));

    pick(&s, 1);
    s.wait_for_screen(&format!("GET {}", fake.url("/me")));
    pick(&s, 1);

    s.wait_for_screen("GET http://prod.invalid/me");
    assert!(s.screen().contains("env: prod"), "{}", s.screen());
}

#[test]
fn the_trace_names_each_scope() {
    let fake = FakeHttp::start();
    let dir = collection(&fake);
    let s = open(&dir, Some(SECRET));
    pick(&s, 1);
    s.wait_for_screen("env: dev");

    s.write(b"t");

    s.wait_for_screen("← env dev");
    assert!(s.screen().contains("← collection"), "{}", s.screen());
}

#[test]
fn the_secret_never_reaches_the_log() {
    let fake = FakeHttp::start();
    fake.route(
        "GET",
        &format!("/a/{SECRET}"),
        Reply::new(200).body("{\"user\":\"demo\"}").require_header(
            "authorization",
            &format!("Bearer {SECRET}"),
            Reply::new(401),
        ),
    );
    let dir = collection(&fake);
    // Sorts first; the secret is in the path, which every request log line carries.
    std::fs::write(
        dir.path().join("a.req.toml"),
        "name = \"In path\"\nurl = \"{{base}}/a/{{token}}\"\n[headers]\nAuthorization = \"Bearer {{token}}\"\n",
    )
    .unwrap();
    let mut s = open(&dir, Some(SECRET));
    pick(&s, 1);
    s.wait_for_screen("env: dev");
    s.write(b"t");
    s.wait_for_screen("← env dev");

    s.write(F5);
    s.wait_for_screen("200 OK");
    let screen = s.screen();
    s.write(b"\x11");
    assert!(s.wait_exit(Duration::from_secs(3)).success(), "clean exit");

    let log = s.log();
    assert!(log.contains("interpolated"), "the step is logged:\n{log}");
    assert!(!log.contains(SECRET), "secret in the log:\n{log}");
    assert!(!screen.contains(SECRET), "secret on screen:\n{screen}");
    assert!(screen.contains("Bearer ••••••"), "masked:\n{screen}");
}

#[test]
fn an_unknown_variable_shows_its_code() {
    let fake = FakeHttp::start();
    let dir = collection(&fake);
    let s = open(&dir, None);

    s.write(DOWN);
    s.write(F5);

    s.wait_for_screen("not sent");
    assert!(s.screen().contains("model::unknown_var"), "{}", s.screen());
    assert!(fake.received().is_empty(), "nothing sent");
}
