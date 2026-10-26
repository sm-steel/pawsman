//! Sending requests through `Net` to `FakeHttp`.
#![expect(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "test helpers: rust rule 1 applies outside tests only"
)]

use std::sync::mpsc;
use std::time::{Duration, Instant};

use pawsman_error::ErrorInfo as _;
use pawsman_model::RequestId;
use pawsman_net::{Body, Net, NetConfig, NetEvent, Outgoing, codes};
use pawsman_test_support::fake_http::{FakeHttp, Reply};

fn start(config: NetConfig) -> (Net, mpsc::Receiver<NetEvent>) {
    let (tx, rx) = mpsc::channel();
    let net = Net::start(config, move |event| {
        let _ = tx.send(event);
    })
    .unwrap();
    (net, rx)
}

fn next(events: &mpsc::Receiver<NetEvent>) -> NetEvent {
    events
        .recv_timeout(Duration::from_secs(5))
        .expect("an event within 5 s")
}

fn id(n: u32) -> RequestId {
    RequestId::new(n)
}

fn get(url: &str) -> Outgoing {
    Outgoing::new("GET", url, &[], None).unwrap()
}

#[test]
fn a_get_returns_status_headers_and_body() {
    let fake = FakeHttp::start();
    fake.route(
        "GET",
        "/users",
        Reply::new(200)
            .header("content-type", "application/json")
            .body("[1,2]"),
    );
    let (net, events) = start(NetConfig::new());

    net.send(
        id(1),
        Outgoing::new(
            "GET",
            &fake.url("/users?page=2"),
            &[("Accept".into(), "application/json".into())],
            None,
        )
        .unwrap(),
    );
    let NetEvent::Done { id: done, response } = next(&events) else {
        panic!("done")
    };

    assert_eq!(done, id(1), "id");
    assert_eq!(
        (response.status(), response.reason()),
        (200, "OK"),
        "status"
    );
    assert_eq!(response.body().preview(), b"[1,2]", "body");
    assert!(
        response
            .headers()
            .iter()
            .any(|(k, v)| k == "content-type" && v == "application/json"),
        "headers: {:?}",
        response.headers()
    );
    assert!(response.elapsed() >= response.ttfb(), "timing");
    let received = fake.received();
    assert_eq!(received[0].path, "/users", "path");
    assert!(
        received[0]
            .headers
            .contains(&("accept".to_owned(), "application/json".to_owned())),
        "header sent: {:?}",
        received[0].headers
    );
    assert!(
        received[0].headers.iter().any(|(k, _)| k == "host"),
        "host set"
    );
}

#[test]
fn a_post_sends_its_body() {
    let fake = FakeHttp::start();
    fake.route("POST", "/echo", Reply::new(201));
    let (net, events) = start(NetConfig::new());

    net.send(
        id(1),
        Outgoing::new("POST", &fake.url("/echo"), &[], Some("{\"a\":1}")).unwrap(),
    );
    let NetEvent::Done { response, .. } = next(&events) else {
        panic!("done")
    };

    assert_eq!(response.status(), 201, "created");
    assert_eq!(fake.received()[0].body, b"{\"a\":1}", "body arrived");
}

#[test]
fn connection_refused_is_a_connect_error() {
    let port = std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let (net, events) = start(NetConfig::new());

    net.send(id(1), get(&format!("http://127.0.0.1:{port}/")));

    let NetEvent::Failed { error, .. } = next(&events) else {
        panic!("failed")
    };
    assert_eq!(error.code(), codes::CONNECT, "{error}");
}

#[test]
fn an_unknown_host_is_a_dns_error() {
    let (net, events) = start(NetConfig::new());

    net.send(id(1), get("http://pawsman-does-not-exist.invalid/"));

    let NetEvent::Failed { error, .. } = next(&events) else {
        panic!("failed")
    };
    assert_eq!(error.code(), codes::DNS, "{error}");
}

#[test]
fn https_to_a_plain_server_is_a_tls_error() {
    let fake = FakeHttp::start();
    let url = fake.url("/").replace("http://", "https://");
    let (net, events) = start(NetConfig::new());

    net.send(id(1), get(&url));

    let NetEvent::Failed { error, .. } = next(&events) else {
        panic!("failed")
    };
    assert_eq!(error.code(), codes::TLS, "{error}");
}

#[test]
fn a_stalled_body_times_out() {
    let fake = FakeHttp::start();
    fake.route(
        "GET",
        "/trickle",
        Reply::new(200).trickle(b"x".to_vec(), Duration::from_millis(50)),
    );
    let (net, events) = start(NetConfig::new().timeout(Duration::from_millis(400)));

    net.send(id(1), get(&fake.url("/trickle")));

    let NetEvent::Failed { error, .. } = next(&events) else {
        panic!("failed")
    };
    assert_eq!(error.code(), codes::TIMEOUT, "{error}");
}

#[test]
fn cancel_mid_body() {
    let fake = FakeHttp::start();
    fake.route(
        "GET",
        "/trickle",
        Reply::new(200).trickle(b"x".to_vec(), Duration::from_millis(50)),
    );
    let (net, events) = start(NetConfig::new());

    net.send(id(7), get(&fake.url("/trickle")));
    std::thread::sleep(Duration::from_millis(200));
    net.cancel(id(7));

    let NetEvent::Cancelled { id: cancelled } = next(&events) else {
        panic!("cancelled")
    };
    assert_eq!(cancelled, id(7), "the right request");
}

#[test]
fn body_above_the_cap_spills_to_a_file() {
    let fake = FakeHttp::start();
    fake.route(
        "GET",
        "/big",
        Reply::new(200).body(vec![b'a'; 1024]).repeat_body(300),
    );
    let (net, events) = start(NetConfig::new().body_cap(64 * 1024));

    net.send(id(1), get(&fake.url("/big")));
    let NetEvent::Done { response, .. } = next(&events) else {
        panic!("done")
    };

    assert_eq!(response.body().len(), 300 * 1024, "full length counted");
    assert_eq!(
        response.body().preview().len(),
        64 * 1024,
        "memory holds the cap"
    );
    let Body::Spilled { file, .. } = response.body() else {
        panic!("spilled")
    };
    assert_eq!(
        std::fs::metadata(&**file).unwrap().len(),
        300 * 1024 - 64 * 1024,
        "the rest on disk"
    );
}

#[test]
fn two_requests_run_at_once() {
    let fake = FakeHttp::start();
    fake.route(
        "GET",
        "/slow",
        Reply::new(200).delay(Duration::from_millis(300)),
    );
    let (net, events) = start(NetConfig::new());

    let start = Instant::now();
    net.send(id(1), get(&fake.url("/slow")));
    net.send(id(2), get(&fake.url("/slow")));
    next(&events);
    next(&events);

    assert!(
        start.elapsed() < Duration::from_millis(550),
        "in parallel: {:?}",
        start.elapsed()
    );
}

#[test]
fn shutdown_cancels_and_joins_quickly() {
    let fake = FakeHttp::start();
    fake.route(
        "GET",
        "/slow",
        Reply::new(200).delay(Duration::from_secs(10)),
    );
    let (net, _events) = start(NetConfig::new());
    net.send(id(1), get(&fake.url("/slow")));
    std::thread::sleep(Duration::from_millis(100));

    let start = Instant::now();
    net.shutdown();

    assert!(
        start.elapsed() < Duration::from_secs(1),
        "joined in {:?}",
        start.elapsed()
    );
}

#[test]
fn an_ipv6_literal_url_sends() {
    let fake = FakeHttp::start_v6();
    fake.route("GET", "/six", Reply::new(200).body("v6"));
    let (net, events) = start(NetConfig::new());

    net.send(id(1), get(&fake.url("/six")));

    let NetEvent::Done { response, .. } = next(&events) else {
        panic!("done: {:?}", fake.url("/six"))
    };
    assert_eq!(response.body().preview(), b"v6", "body");
    let host = fake.received()[0]
        .headers
        .iter()
        .find(|(k, _)| k == "host")
        .map(|(_, v)| v.clone());
    assert_eq!(
        host.as_deref().map(|h| h.starts_with("[::1]:")),
        Some(true),
        "host header {host:?}"
    );
}

#[test]
fn a_post_without_a_body_says_content_length_zero() {
    let fake = FakeHttp::start();
    fake.route("POST", "/logout", Reply::new(204));
    let (net, events) = start(NetConfig::new());

    net.send(
        id(1),
        Outgoing::new("POST", &fake.url("/logout"), &[], None).unwrap(),
    );
    next(&events);

    let headers = &fake.received()[0].headers;
    assert!(
        headers.contains(&("content-length".to_owned(), "0".to_owned())),
        "strict servers answer 411 without it: {headers:?}"
    );
}

#[test]
fn a_get_without_a_body_sends_no_content_length() {
    let fake = FakeHttp::start();
    fake.route("GET", "/x", Reply::new(200));
    let (net, events) = start(NetConfig::new());

    net.send(id(1), get(&fake.url("/x")));
    next(&events);

    assert!(
        !fake.received()[0]
            .headers
            .iter()
            .any(|(k, _)| k == "content-length"),
        "no body, no length on GET"
    );
}
