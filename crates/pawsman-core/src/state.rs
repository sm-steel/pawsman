//! The engine's state: the collection, the active environment and every request's status.
//! `apply` is a pure reducer that returns the effects to run; snapshots are what the UI draws.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Instant, SystemTime};

use pawsman_error::ErrorInfo;
use pawsman_model::{Collection, EntryKind, Environment, ModelError, RequestId};
use pawsman_net::{NetError, NetEvent, Outgoing, Response};
use tracing::{debug, info};

use crate::notice::Notice;
use crate::resolve::{self, Context, Preview};
use crate::wire;

/// Where a request stands.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum RequestStatus {
    /// Never sent in this session.
    Idle,
    /// In flight since `since`.
    Sending {
        /// When it was sent.
        since: Instant,
    },
    /// Finished.
    Done(Arc<Response>),
    /// Failed.
    Failed(NetError),
    /// Not sent: a variable, secret or body file could not be resolved.
    Unresolved(Notice),
    /// Cancelled.
    Cancelled,
}

static IDLE: RequestStatus = RequestStatus::Idle;

/// What the UI draws: an immutable copy of the state.
#[derive(Debug, Clone)]
pub struct Snapshot {
    collection: Option<Arc<Collection>>,
    load_error: Option<ModelError>,
    active_env: Option<String>,
    context: Arc<Context>,
    statuses: BTreeMap<RequestId, RequestStatus>,
    notices: Vec<Notice>,
    generation: u64,
}

impl Snapshot {
    /// The open collection.
    #[must_use]
    pub fn collection(&self) -> Option<&Collection> {
        self.collection.as_deref()
    }

    /// Why the collection did not open.
    #[must_use]
    pub fn load_error(&self) -> Option<&ModelError> {
        self.load_error.as_ref()
    }

    /// The collection's environments, broken ones included.
    #[must_use]
    pub fn environments(&self) -> &[Environment] {
        self.collection().map_or(&[], Collection::environments)
    }

    /// The environment requests resolve in.
    #[must_use]
    pub fn active_env(&self) -> Option<&str> {
        self.active_env.as_deref()
    }

    /// Request `id` as it would be sent now, secrets masked; `None` for no such request.
    #[must_use]
    pub fn preview(&self, id: RequestId) -> Option<Result<Preview, Notice>> {
        let collection = self.collection()?;
        resolve::preview(collection, id, self.active_env(), &self.context)
            .map(|r| r.map_err(|e| Notice::from_error(&e)))
    }

    /// The status of request `id`.
    #[must_use]
    pub fn status(&self, id: RequestId) -> &RequestStatus {
        self.statuses.get(&id).unwrap_or(&IDLE)
    }

    /// Whether any request is in flight.
    #[must_use]
    pub fn any_sending(&self) -> bool {
        self.statuses
            .values()
            .any(|s| matches!(s, RequestStatus::Sending { .. }))
    }

    /// Notices raised since the previous snapshot.
    #[must_use]
    pub fn notices(&self) -> &[Notice] {
        &self.notices
    }

    /// Bumps on every change.
    #[must_use]
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// The path of a node a tree operation just created or renamed, delivered once.
    #[must_use]
    pub fn created(&self) -> Option<&Path> {
        None
    }
}

/// An input to the reducer.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum Event {
    /// Send request `id`; built-ins take the time `at` and random values from `seed`.
    Send {
        /// The request.
        id: RequestId,
        /// The wall clock for `$timestamp` and `$isoDate`.
        at: SystemTime,
        /// The seed for `$uuid`, `$randomInt` and the multipart boundary.
        seed: u64,
    },
    /// Cancel request `id`.
    Cancel(RequestId),
    /// Resolve in this environment from now on; `None` for none.
    SelectEnv(Option<String>),
    /// The network reported something.
    Net(NetEvent),
    /// The collection was read again from disk.
    Reloaded(Result<Collection, ModelError>),
    /// A tree operation created or renamed the node at this path.
    Created(PathBuf),
}

/// Work the engine runs after a change.
#[derive(Debug)]
#[non_exhaustive]
pub enum Effect {
    /// Send this request.
    Send(RequestId, Box<Outgoing>),
    /// Cancel this request.
    Cancel(RequestId),
}

/// The engine's state.
#[derive(Debug)]
pub struct AppState {
    collection: Option<Arc<Collection>>,
    load_error: Option<ModelError>,
    active_env: Option<String>,
    context: Arc<Context>,
    statuses: BTreeMap<RequestId, RequestStatus>,
    notices: Vec<Notice>,
    generation: u64,
}

impl AppState {
    /// State for a collection that loaded, or did not; its issues become notices.
    #[must_use]
    pub fn new(load: Result<Collection, ModelError>, context: Context) -> Self {
        let (collection, load_error) = match load {
            Ok(collection) => (Some(Arc::new(collection)), None),
            Err(err) => (None, Some(err)),
        };
        let notices = collection
            .as_ref()
            .map(|c| c.issues().iter().map(|i| Notice::from_error(i)).collect())
            .unwrap_or_default();
        Self {
            collection,
            load_error,
            active_env: None,
            context: Arc::new(context),
            statuses: BTreeMap::new(),
            notices,
            generation: 0,
        }
    }

    /// State without a collection.
    #[must_use]
    pub fn empty() -> Self {
        Self {
            collection: None,
            load_error: None,
            active_env: None,
            context: Arc::new(Context::default()),
            statuses: BTreeMap::new(),
            notices: Vec::new(),
            generation: 0,
        }
    }

    /// Bumps on every change.
    #[must_use]
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// Raises `notice` with the next snapshot.
    pub fn notify(&mut self, notice: Notice) {
        self.notice(notice);
    }

    /// Applies `event` at `now`; returns what to run.
    pub fn apply(&mut self, event: Event, now: Instant) -> Vec<Effect> {
        match event {
            Event::Send { id, at, seed } => self.send(id, now, at, seed),
            Event::Cancel(id) => {
                if matches!(self.statuses.get(&id), Some(RequestStatus::Sending { .. })) {
                    vec![Effect::Cancel(id)]
                } else {
                    Vec::new()
                }
            }
            Event::SelectEnv(env) => {
                self.select_env(env);
                Vec::new()
            }
            Event::Reloaded(_) | Event::Created(_) => Vec::new(),
            Event::Net(event) => {
                let (id, status) = match event {
                    NetEvent::Done { id, response } => {
                        (id, RequestStatus::Done(Arc::new(response)))
                    }
                    NetEvent::Failed { id, error } => (id, RequestStatus::Failed(error)),
                    NetEvent::Cancelled { id } => (id, RequestStatus::Cancelled),
                    _ => return Vec::new(),
                };
                self.set(id, status);
                Vec::new()
            }
        }
    }

    fn select_env(&mut self, env: Option<String>) {
        if let Some(name) = env.as_deref() {
            let found = self
                .collection
                .as_ref()
                .and_then(|c| c.environments().iter().find(|e| e.name() == name));
            match found {
                None => {
                    self.notice(Notice::warn(format!("no environment `{name}`")));
                    return;
                }
                Some(e) if let Some(err) = e.error() => {
                    let notice = Notice::from_error(err);
                    self.notice(notice);
                    return;
                }
                Some(_) => {}
            }
        }
        info!(
            env = env.as_deref().unwrap_or("none"),
            "environment selected"
        );
        self.active_env = env;
        self.generation += 1;
    }

    fn send(&mut self, id: RequestId, now: Instant, at: SystemTime, seed: u64) -> Vec<Effect> {
        if matches!(self.statuses.get(&id), Some(RequestStatus::Sending { .. })) {
            return Vec::new();
        }
        let Some(collection) = self.collection.clone() else {
            return Vec::new();
        };
        let Some(entry) = collection.entry(id) else {
            return Vec::new();
        };
        if let EntryKind::Invalid(err) = entry.kind() {
            self.notice(Notice::from_error(err));
            return Vec::new();
        }
        let resolved = resolve::resolve(
            &collection,
            id,
            self.active_env.as_deref(),
            &self.context,
            at,
            seed,
        );
        let (resolved, trace) = match resolved {
            None => return Vec::new(),
            Some(Ok(done)) => done,
            Some(Err(err)) => return self.unresolved(id, &err),
        };
        let mut sources: Vec<String> = Vec::new();
        for source in trace.entries().iter().map(|e| e.source().to_string()) {
            if !sources.contains(&source) {
                sources.push(source);
            }
        }
        debug!(%id, vars = trace.entries().len(), scopes = ?sources, "interpolated");
        let wire = match wire::encode(&resolved, collection.root(), seed) {
            Ok(wire) => wire,
            Err(err) => return self.unresolved(id, &err),
        };
        let outgoing = Outgoing::new(
            resolved.method(),
            &wire.url,
            &wire.headers,
            wire.body,
            resolved.redactor().clone(),
        );
        match outgoing {
            Ok(outgoing) => {
                self.set(id, RequestStatus::Sending { since: now });
                vec![Effect::Send(id, Box::new(outgoing))]
            }
            Err(err) => {
                self.set(id, RequestStatus::Failed(err));
                Vec::new()
            }
        }
    }

    fn unresolved(&mut self, id: RequestId, err: &dyn ErrorInfo) -> Vec<Effect> {
        info!(%id, code = %err.code(), "request not resolved");
        self.set(id, RequestStatus::Unresolved(Notice::from_error(err)));
        Vec::new()
    }

    fn set(&mut self, id: RequestId, status: RequestStatus) {
        self.statuses.insert(id, status);
        self.generation += 1;
    }

    fn notice(&mut self, notice: Notice) {
        self.notices.push(notice);
        self.generation += 1;
    }

    /// A snapshot of the state; takes the pending notices.
    pub fn snapshot(&mut self) -> Arc<Snapshot> {
        Arc::new(Snapshot {
            collection: self.collection.clone(),
            load_error: self.load_error.clone(),
            active_env: self.active_env.clone(),
            context: Arc::clone(&self.context),
            statuses: self.statuses.clone(),
            notices: std::mem::take(&mut self.notices),
            generation: self.generation,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use pawsman_model::SourceFile;
    use pawsman_model::trace::TraceValue;

    use super::*;
    use crate::env::EnvSnapshot;

    fn id(n: u32) -> RequestId {
        RequestId::new(n)
    }

    fn collection(files: &[(&str, &str)]) -> Collection {
        Collection::from_files(
            Path::new("/c"),
            std::iter::once(SourceFile::new(
                "pawsman.toml".into(),
                b"format = 1\nname = \"C\"\n".to_vec(),
            ))
            .chain(
                files
                    .iter()
                    .map(|(n, t)| SourceFile::new(PathBuf::from(n), t.as_bytes().to_vec())),
            )
            .collect(),
        )
        .unwrap()
    }

    fn demo() -> Collection {
        collection(&[("a.req.toml", "name = \"A\"\nurl = \"http://h/a\"\n")])
    }

    fn now() -> Instant {
        Instant::now()
    }

    fn send(n: u32) -> Event {
        Event::Send {
            id: id(n),
            at: std::time::UNIX_EPOCH,
            seed: 1,
        }
    }

    fn sent_url(effects: &[Effect]) -> Option<String> {
        match effects {
            [Effect::Send(_, outgoing)] => Some(outgoing.uri_for_tests()),
            _ => None,
        }
    }

    const ENVS: &[(&str, &str)] = &[
        ("u.req.toml", "name = \"U\"\nurl = \"{{base}}/u\"\n"),
        ("envs/dev.env.toml", "[vars]\nbase = \"http://dev\"\n"),
        ("envs/prod.env.toml", "[vars]\nbase = \"http://prod\"\n"),
        ("envs/broken.env.toml", "vars = 1\n"),
    ];

    #[test]
    fn sending_resolves_against_the_active_environment() {
        let mut s = AppState::new(Ok(collection(ENVS)), Context::default());

        s.apply(Event::SelectEnv(Some("dev".into())), now());
        let dev = s.apply(send(1), now());
        s.apply(Event::Net(NetEvent::Cancelled { id: id(1) }), now());
        s.apply(Event::SelectEnv(Some("prod".into())), now());
        let prod = s.apply(send(1), now());

        assert_eq!(sent_url(&dev).as_deref(), Some("http://dev/u"), "dev");
        assert_eq!(sent_url(&prod).as_deref(), Some("http://prod/u"), "prod");
        assert_eq!(s.snapshot().active_env(), Some("prod"), "active");
    }

    #[test]
    fn an_unknown_var_is_unresolved_and_sends_nothing() {
        let mut s = AppState::new(Ok(collection(ENVS)), Context::default());

        let effects = s.apply(send(1), now());

        assert!(effects.is_empty(), "nothing sent");
        assert!(
            matches!(s.snapshot().status(id(1)), RequestStatus::Unresolved(n) if n.text().starts_with("model::unknown_var")),
            "{:?}",
            s.snapshot().status(id(1))
        );
    }

    #[test]
    fn a_missing_secret_is_unresolved_with_the_env_name() {
        let files = [
            ("u.req.toml", "name = \"U\"\nurl = \"http://h/{{token}}\"\n"),
            ("envs/dev.env.toml", "secrets = [\"token\"]\n"),
        ];
        let mut s = AppState::new(Ok(collection(&files)), Context::default());
        s.apply(Event::SelectEnv(Some("dev".into())), now());

        let effects = s.apply(send(1), now());

        assert!(effects.is_empty(), "nothing sent");
        assert!(
            matches!(s.snapshot().status(id(1)), RequestStatus::Unresolved(n) if n.text().contains("model::missing_secret") && n.text().contains("env dev")),
            "{:?}",
            s.snapshot().status(id(1))
        );
    }

    #[test]
    fn a_secret_from_the_process_env_is_used_and_masked_in_preview() {
        let files = [
            (
                "u.req.toml",
                "name = \"U\"\nurl = \"http://h/u\"\n[headers]\nAuthorization = \"Bearer {{token}}\"\n",
            ),
            ("envs/dev.env.toml", "secrets = [\"token\"]\n"),
        ];
        let env = EnvSnapshot::from_pairs([("PAWSMAN_SECRET_TOKEN", "s3cr3t")]);
        let mut s = AppState::new(Ok(collection(&files)), Context::new(vec![], env));
        s.apply(Event::SelectEnv(Some("dev".into())), now());

        let effects = s.apply(send(1), now());
        let preview = s.snapshot().preview(id(1)).unwrap().unwrap();

        let [Effect::Send(_, outgoing)] = effects.as_slice() else {
            panic!("{effects:?}");
        };
        assert_eq!(
            outgoing.header_for_tests("authorization").as_deref(),
            Some("Bearer s3cr3t"),
            "on the wire"
        );
        assert_eq!(
            preview.headers(),
            &[("Authorization".to_owned(), "Bearer ••••••".to_owned())],
            "masked"
        );
        assert!(
            preview
                .trace()
                .entries()
                .iter()
                .all(|e| e.value() == &TraceValue::Secret),
            "{:?}",
            preview.trace()
        );
        assert_eq!(preview.env(), Some("dev"), "env named");
    }

    #[test]
    fn dotenv_and_globals_resolve() {
        let files = [
            ("u.req.toml", "name = \"U\"\nurl = \"{{host}}/{{token}}\"\n"),
            ("envs/dev.env.toml", "secrets = [\"token\"]\n"),
            (".env", "token=from-dotenv\n"),
        ];
        let context = Context::new(
            vec![("host".into(), "http://global".into())],
            EnvSnapshot::default(),
        );
        let mut s = AppState::new(Ok(collection(&files)), context);
        s.apply(Event::SelectEnv(Some("dev".into())), now());

        let effects = s.apply(send(1), now());

        assert_eq!(
            sent_url(&effects).as_deref(),
            Some("http://global/from-dotenv")
        );
    }

    #[test]
    fn selecting_a_broken_or_unknown_env_is_a_notice() {
        let mut s = AppState::new(Ok(collection(ENVS)), Context::default());

        s.apply(Event::SelectEnv(Some("broken".into())), now());
        s.apply(Event::SelectEnv(Some("nope".into())), now());
        let snap = s.snapshot();

        assert_eq!(snap.active_env(), None, "unchanged");
        assert_eq!(snap.notices().len(), 2, "{:?}", snap.notices());
    }

    #[test]
    fn preview_uses_placeholders_for_builtins() {
        let files = [(
            "u.req.toml",
            "name = \"U\"\nurl = \"http://h/{{$uuid}}\"\n[query]\nq = \"a b\"\n",
        )];
        let s = AppState::new(Ok(collection(&files)), Context::default()).snapshot();

        let preview = s.preview(id(1)).unwrap().unwrap();

        assert_eq!(preview.url(), "http://h/‹uuid›?q=a%20b");
        assert!(s.preview(id(9)).is_none(), "no such request");
    }

    #[test]
    fn preview_describes_every_body_kind() {
        let files = [
            (
                "a.req.toml",
                "name = \"A\"\nmethod = \"post\"\nurl = \"http://h\"\n[body.form]\nq = \"a b\"\n",
            ),
            (
                "b.req.toml",
                "name = \"B\"\nurl = \"http://h\"\n[[body.multipart]]\nname = \"n\"\nvalue = \"v\"\n[[body.multipart]]\nname = \"f\"\nfile = \"up.png\"\n",
            ),
            (
                "c.req.toml",
                "name = \"C\"\nurl = \"http://h\"\n[body]\nfile = \"up.bin\"\n[[capture]]\nvar = \"x\"\n",
            ),
        ];
        let s = AppState::new(Ok(collection(&files)), Context::default()).snapshot();
        let preview = |n| s.preview(id(n)).unwrap().unwrap();

        assert_eq!(
            (preview(1).method(), preview(1).body_text()),
            ("POST", Some("q=a+b"))
        );
        assert_eq!(
            preview(2).body_text(),
            Some("n: v\nf: @up.png"),
            "multipart"
        );
        assert_eq!(preview(3).body_text(), Some("@up.bin"), "file");
        assert!(
            preview(3).has_m2_parts() && !preview(1).has_m2_parts(),
            "m2 parts"
        );
        assert!(preview(1).trace().entries().is_empty(), "no variables");
    }

    fn done_for(s: &mut AppState, n: u32) {
        s.apply(send(n), now());
        let response = Response::for_tests(200, b"ok", Duration::from_millis(5));
        s.apply(
            Event::Net(NetEvent::Done {
                id: id(n),
                response,
            }),
            now(),
        );
    }

    #[test]
    fn reload_keeps_statuses_of_surviving_ids_and_drops_the_rest() {
        let files = [
            ("a.req.toml", "name = \"A\"\nurl = \"http://h/a\"\n"),
            ("b.req.toml", "name = \"B\"\nurl = \"http://h/b\"\n"),
        ];
        let mut s = AppState::new(Ok(collection(&files)), Context::default());
        done_for(&mut s, 1);
        done_for(&mut s, 2);
        let g = s.snapshot().generation();

        s.apply(Event::Reloaded(Ok(collection(&files[..1]))), now());
        let snap = s.snapshot();

        assert!(snap.generation() > g, "a change");
        assert!(
            matches!(snap.status(id(1)), RequestStatus::Done(_)),
            "a kept its response"
        );
        assert!(s.statuses.get(&id(2)).is_none(), "b's status dropped");
        assert_eq!(
            snap.collection().unwrap().entries().len(),
            1,
            "new collection"
        );
    }

    #[test]
    fn reload_clears_an_env_that_broke() {
        let mut s = AppState::new(Ok(collection(ENVS)), Context::default());
        s.apply(Event::SelectEnv(Some("dev".into())), now());
        s.snapshot();
        let broken = [ENVS[0], ("envs/dev.env.toml", "vars = 1\n")];

        s.apply(Event::Reloaded(Ok(collection(&broken))), now());
        let snap = s.snapshot();

        assert_eq!(snap.active_env(), None, "cleared");
        assert!(
            snap.notices().iter().any(|n| n.text().contains("dev")),
            "{:?}",
            snap.notices()
        );
    }

    #[test]
    fn reload_keeps_a_working_env() {
        let mut s = AppState::new(Ok(collection(ENVS)), Context::default());
        s.apply(Event::SelectEnv(Some("dev".into())), now());

        s.apply(Event::Reloaded(Ok(collection(ENVS))), now());

        assert_eq!(s.snapshot().active_env(), Some("dev"), "kept");
    }

    #[test]
    fn a_failed_reload_keeps_the_collection_with_a_notice() {
        let mut s = AppState::new(Ok(demo()), Context::default());
        s.snapshot();

        s.apply(
            Event::Reloaded(Err(ModelError::invalid_file(
                Path::new("pawsman.toml"),
                "expected `=`",
            ))),
            now(),
        );
        let snap = s.snapshot();

        assert!(snap.collection().is_some(), "kept");
        assert!(
            snap.notices()
                .iter()
                .any(|n| n.text().starts_with("model::invalid_file")),
            "{:?}",
            snap.notices()
        );
    }

    #[test]
    fn new_issues_on_reload_become_notices_once() {
        let mut s = AppState::new(Ok(demo()), Context::default());
        s.snapshot();
        let bad = [
            ("a.req.toml", "name = \"A\"\nurl = \"http://h/a\"\n"),
            (".env", "bad line\n"),
        ];

        s.apply(Event::Reloaded(Ok(collection(&bad))), now());
        let first = s.snapshot().notices().len();
        s.apply(Event::Reloaded(Ok(collection(&bad))), now());
        let second = s.snapshot().notices().len();

        assert_eq!((first, second), (1, 0), "only new issues");
    }

    #[test]
    fn created_is_delivered_once() {
        let mut s = AppState::new(Ok(demo()), Context::default());

        s.apply(Event::Created("a.req.toml".into()), now());
        let first = s.snapshot();
        let second = s.snapshot();

        assert_eq!(first.created(), Some(Path::new("a.req.toml")), "delivered");
        assert_eq!(second.created(), None, "once");
    }

    #[test]
    fn collection_issues_become_notices() {
        let mut s = AppState::new(
            Ok(collection(&[(".env", "bad line\n")])),
            Context::default(),
        );

        assert_eq!(s.snapshot().notices().len(), 1, "the bad .env line");
    }

    #[test]
    fn send_starts_and_the_net_event_finishes() {
        let mut s = AppState::new(Ok(demo()), Context::default());

        let effects = s.apply(send(1), now());
        let sending = matches!(s.snapshot().status(id(1)), RequestStatus::Sending { .. });
        let response = Response::for_tests(200, b"ok", Duration::from_millis(5));
        s.apply(
            Event::Net(NetEvent::Done {
                id: id(1),
                response,
            }),
            now(),
        );

        assert!(
            matches!(effects.as_slice(), [Effect::Send(i, _)] if *i == id(1)),
            "{effects:?}"
        );
        assert!(sending, "sending after Send");
        assert!(
            matches!(s.snapshot().status(id(1)), RequestStatus::Done(r) if r.status() == 200),
            "done"
        );
    }

    #[test]
    fn a_second_send_while_sending_does_nothing() {
        let mut s = AppState::new(Ok(demo()), Context::default());
        s.apply(send(1), now());

        let effects = s.apply(send(1), now());

        assert!(effects.is_empty(), "no duplicate request");
    }

    #[test]
    fn cancel_while_sending_asks_the_net_and_cancelled_ends_it() {
        let mut s = AppState::new(Ok(demo()), Context::default());
        s.apply(send(1), now());

        let effects = s.apply(Event::Cancel(id(1)), now());
        s.apply(Event::Net(NetEvent::Cancelled { id: id(1) }), now());

        assert!(
            matches!(effects.as_slice(), [Effect::Cancel(i)] if *i == id(1)),
            "{effects:?}"
        );
        assert!(
            matches!(s.snapshot().status(id(1)), RequestStatus::Cancelled),
            "cancelled"
        );
    }

    #[test]
    fn an_invalid_url_fails_without_a_request() {
        let mut s = AppState::new(
            Ok(collection(&[(
                "a.req.toml",
                "name = \"A\"\nurl = \"not a url\"\n",
            )])),
            Context::default(),
        );

        let effects = s.apply(send(1), now());
        let snap = s.snapshot();

        assert!(effects.is_empty(), "nothing sent");
        assert!(
            matches!(snap.status(id(1)), RequestStatus::Failed(e) if e.code() == pawsman_net::codes::INVALID_REQUEST),
            "failed at once: {:?}",
            snap.status(id(1))
        );
    }

    #[test]
    fn an_invalid_entry_gives_a_notice_and_no_request() {
        let mut s = AppState::new(
            Ok(collection(&[("bad.req.toml", "name = ")])),
            Context::default(),
        );

        let effects = s.apply(send(1), now());
        let snap = s.snapshot();

        assert!(effects.is_empty(), "nothing sent");
        assert!(
            snap.notices()
                .iter()
                .any(|n| n.text().contains("model::invalid_file")),
            "notice: {:?}",
            snap.notices()
        );
    }

    #[test]
    fn notices_are_delivered_once() {
        let mut s = AppState::new(
            Ok(collection(&[("bad.req.toml", "name = ")])),
            Context::default(),
        );
        s.apply(send(1), now());

        let first = s.snapshot();
        let second = s.snapshot();

        assert_eq!(first.notices().len(), 1, "delivered");
        assert!(second.notices().is_empty(), "not repeated");
    }

    #[test]
    fn generation_bumps_on_every_change_only() {
        let mut s = AppState::new(Ok(demo()), Context::default());
        let g0 = s.snapshot().generation();

        s.apply(Event::Cancel(id(1)), now());
        let g1 = s.snapshot().generation();
        s.apply(send(1), now());

        assert_eq!(g0, g1, "cancelling an idle request changes nothing");
        assert!(s.snapshot().generation() > g1, "sending does");
    }

    #[test]
    fn a_failed_load_has_no_collection() {
        let mut s = AppState::new(
            Err(ModelError::not_a_collection(Path::new("/x"))),
            Context::default(),
        );

        let snap = s.snapshot();

        assert!(
            snap.collection().is_none() && snap.load_error().is_some(),
            "error kept"
        );
        assert!(s.apply(send(1), now()).is_empty(), "nothing to send");
    }
}
