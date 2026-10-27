//! `fixtures/demo` stays a valid collection: the manual runs and the docs depend on it.

use std::path::Path;

use pawsman_core::resolve::Context;
use pawsman_core::state::AppState;
use pawsman_core::store;
use pawsman_model::{EntryKind, Request};

fn demo() -> pawsman_model::Collection {
    store::load(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/demo")).unwrap()
}

#[test]
fn the_demo_collection_loads_with_one_deliberately_broken_file() {
    let c = demo();

    let broken: Vec<String> = c
        .entries()
        .iter()
        .filter(|e| matches!(e.kind(), EntryKind::Invalid(_)))
        .map(|e| e.path().display().to_string())
        .collect();
    assert_eq!(broken, ["misc/broken.req.toml"], "only the deliberate one");
    assert!(c.issues().is_empty(), "{:?}", c.issues());
    let envs: Vec<&str> = c
        .environments()
        .iter()
        .map(pawsman_model::Environment::name)
        .collect();
    assert_eq!(envs, ["dev", "prod"], "environments");
    assert_eq!(c.dotenv().len(), 1, "the placeholder token");
}

#[test]
fn the_sidecar_body_is_long_enough_to_stay_a_sidecar() {
    let c = demo();

    let create = c
        .entries()
        .iter()
        .find(|e| e.path() == Path::new("users/create.req.toml"))
        .and_then(|e| c.request(e.id()))
        .and_then(Request::body_text)
        .expect("create has its sidecar body");

    assert!(
        create.lines().count() > 40,
        "{} lines",
        create.lines().count()
    );
}

#[test]
fn every_demo_request_but_two_resolves_in_dev() {
    let c = demo();
    let ids: Vec<_> = c.entries().iter().map(pawsman_model::Entry::id).collect();
    let mut state = AppState::new(Ok(c), Context::default());
    state.apply(
        pawsman_core::state::Event::SelectEnv(Some("dev".into())),
        std::time::Instant::now(),
    );
    let snap = state.snapshot();

    let failing: Vec<String> = ids
        .iter()
        .filter(|id| matches!(snap.preview(**id), Some(Err(_)) | None))
        .filter_map(|id| snap.collection()?.entry(*id))
        .map(|e| e.path().display().to_string())
        .collect();

    assert_eq!(
        failing,
        ["misc/broken.req.toml", "template.req.toml"],
        "the broken file and the unresolved variable, nothing else"
    );
}
