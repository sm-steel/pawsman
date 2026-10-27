//! The engine's state: the collection and every request's status. `apply` is a pure reducer that
//! returns the effects to run; snapshots are what the UI draws.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;

#[cfg(test)]
use pawsman_model::SourceFile;
use pawsman_model::{Collection, EntryKind, ModelError, RequestId};
use pawsman_net::{NetError, NetEvent, Outgoing, Response};

use crate::notice::Notice;

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
    /// Cancelled.
    Cancelled,
}

static IDLE: RequestStatus = RequestStatus::Idle;

/// What the UI draws: an immutable copy of the state.
#[derive(Debug, Clone)]
pub struct Snapshot {
    collection: Option<Arc<Collection>>,
    load_error: Option<ModelError>,
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
}

/// An input to the reducer.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum Event {
    /// Send request `id`.
    Send(RequestId),
    /// Cancel request `id`.
    Cancel(RequestId),
    /// The network reported something.
    Net(NetEvent),
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
    statuses: BTreeMap<RequestId, RequestStatus>,
    notices: Vec<Notice>,
    generation: u64,
}

impl AppState {
    /// State for a collection that loaded, or did not.
    #[must_use]
    pub fn new(load: Result<Collection, ModelError>) -> Self {
        let (collection, load_error) = match load {
            Ok(collection) => (Some(Arc::new(collection)), None),
            Err(err) => (None, Some(err)),
        };
        Self {
            collection,
            load_error,
            statuses: BTreeMap::new(),
            notices: Vec::new(),
            generation: 0,
        }
    }

    /// State without a collection.
    #[must_use]
    pub fn empty() -> Self {
        Self {
            collection: None,
            load_error: None,
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

    /// Applies `event` at `now`; returns what to run.
    pub fn apply(&mut self, event: Event, now: Instant) -> Vec<Effect> {
        match event {
            Event::Send(id) => self.send(id, now),
            Event::Cancel(id) => {
                if matches!(self.statuses.get(&id), Some(RequestStatus::Sending { .. })) {
                    vec![Effect::Cancel(id)]
                } else {
                    Vec::new()
                }
            }
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

    fn send(&mut self, id: RequestId, now: Instant) -> Vec<Effect> {
        if matches!(self.statuses.get(&id), Some(RequestStatus::Sending { .. })) {
            return Vec::new();
        }
        let Some(collection) = self.collection.clone() else {
            return Vec::new();
        };
        let Some(entry) = collection.entries().iter().find(|e| e.id() == id) else {
            return Vec::new();
        };
        let request = match entry.kind() {
            EntryKind::Request(request) => request,
            EntryKind::Invalid(err) => {
                self.notice(Notice::from_error(err));
                return Vec::new();
            }
            _ => return Vec::new(),
        };
        let outgoing = Outgoing::new(
            request.method(),
            request.url(),
            request.headers(),
            request.body_text(),
        );
        match outgoing {
            Ok(outgoing) => {
                if request.has_template() {
                    self.notice(Notice::warn(
                        "variables are not resolved yet; sent as written",
                    ));
                }
                self.set(id, RequestStatus::Sending { since: now });
                vec![Effect::Send(id, Box::new(outgoing))]
            }
            Err(err) => {
                self.set(id, RequestStatus::Failed(err));
                Vec::new()
            }
        }
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
            statuses: self.statuses.clone(),
            notices: std::mem::take(&mut self.notices),
            generation: self.generation,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};
    use std::time::Duration;

    use pawsman_error::ErrorInfo as _;

    use super::*;

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

    #[test]
    fn send_starts_and_the_net_event_finishes() {
        let mut s = AppState::new(Ok(demo()));

        let effects = s.apply(Event::Send(id(1)), now());
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
        let mut s = AppState::new(Ok(demo()));
        s.apply(Event::Send(id(1)), now());

        let effects = s.apply(Event::Send(id(1)), now());

        assert!(effects.is_empty(), "no duplicate request");
    }

    #[test]
    fn cancel_while_sending_asks_the_net_and_cancelled_ends_it() {
        let mut s = AppState::new(Ok(demo()));
        s.apply(Event::Send(id(1)), now());

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
        let mut s = AppState::new(Ok(collection(&[(
            "a.req.toml",
            "name = \"A\"\nurl = \"not a url\"\n",
        )])));

        let effects = s.apply(Event::Send(id(1)), now());
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
        let mut s = AppState::new(Ok(collection(&[("bad.req.toml", "name = ")])));

        let effects = s.apply(Event::Send(id(1)), now());
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
    fn a_template_sends_with_a_notice() {
        let mut s = AppState::new(Ok(collection(&[(
            "a.req.toml",
            "name = \"A\"\nurl = \"http://h/{{id}}\"\n",
        )])));

        let effects = s.apply(Event::Send(id(1)), now());
        let snap = s.snapshot();

        assert_eq!(effects.len(), 1, "sent as is");
        assert!(
            snap.notices()
                .iter()
                .any(|n| n.text().contains("variables")),
            "notice: {:?}",
            snap.notices()
        );
    }

    #[test]
    fn notices_are_delivered_once() {
        let mut s = AppState::new(Ok(collection(&[("bad.req.toml", "name = ")])));
        s.apply(Event::Send(id(1)), now());

        let first = s.snapshot();
        let second = s.snapshot();

        assert_eq!(first.notices().len(), 1, "delivered");
        assert!(second.notices().is_empty(), "not repeated");
    }

    #[test]
    fn generation_bumps_on_every_change_only() {
        let mut s = AppState::new(Ok(demo()));
        let g0 = s.snapshot().generation();

        s.apply(Event::Cancel(id(1)), now());
        let g1 = s.snapshot().generation();
        s.apply(Event::Send(id(1)), now());

        assert_eq!(g0, g1, "cancelling an idle request changes nothing");
        assert!(s.snapshot().generation() > g1, "sending does");
    }

    #[test]
    fn a_failed_load_has_no_collection() {
        let mut s = AppState::new(Err(ModelError::not_a_collection(Path::new("/x"))));

        let snap = s.snapshot();

        assert!(
            snap.collection().is_none() && snap.load_error().is_some(),
            "error kept"
        );
        assert!(
            s.apply(Event::Send(id(1)), now()).is_empty(),
            "nothing to send"
        );
    }
}
