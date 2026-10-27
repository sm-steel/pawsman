//! `pawsman-fake [--port N]`: the demo API for `fixtures/demo`, on `127.0.0.1:8787` by default.
#![forbid(unsafe_code)]

use std::io::Write as _;
use std::time::Duration;

use pawsman_test_support::fake_http::{FakeHttp, Reply};

#[expect(
    clippy::disallowed_methods,
    reason = "a dev tool: reads its arguments and prints where it listens"
)]
fn main() {
    let args: Vec<String> = std::env::args().collect();
    let port = args
        .iter()
        .position(|a| a == "--port")
        .and_then(|i| args.get(i + 1))
        .and_then(|p| p.parse().ok())
        .unwrap_or(8787);
    let fake = FakeHttp::start_on(port);
    let json = |body: &str| {
        Reply::new(200)
            .header("content-type", "application/json")
            .body(body)
    };
    fake.route(
        "GET",
        "/users",
        json(r#"[{"id":1,"name":"Ada"},{"id":2,"name":"Grace"}]"#),
    );
    fake.route(
        "GET",
        "/slow",
        json(r#"{"slow":true}"#).delay(Duration::from_secs(3)),
    );
    fake.route(
        "GET",
        "/big",
        Reply::new(200)
            .body(vec![b'a'; 1024 * 1024])
            .repeat_body(20),
    );
    fake.route("POST", "/echo", Reply::new(201).body("created"));
    fake.route(
        "GET",
        "/me",
        json(r#"{"user":"demo"}"#).require_header(
            "authorization",
            "Bearer demo-token",
            Reply::new(401).body("missing or wrong token"),
        ),
    );
    fake.route(
        "GET",
        "/bytes",
        Reply::new(200).body((0..=255u8).collect::<Vec<_>>()),
    );
    let _ = writeln!(std::io::stdout(), "listening on {}", fake.url(""));
    loop {
        std::thread::park();
    }
}
