//! HTTP for pawsman, on a private tokio runtime thread behind a sync handle. No `async fn` or
//! tokio type appears in this crate's public API.
#![forbid(unsafe_code)]

pub mod codes;
mod error;
mod outgoing;
mod response;
mod runtime;

pub use error::{NetError, Phase};
pub use outgoing::Outgoing;
pub use response::{Body, Response};
pub use runtime::{Net, NetConfig, NetEvent};
