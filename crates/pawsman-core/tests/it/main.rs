//! The engine against the fake HTTP server.
#![expect(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "test helpers: rust rule 1 applies outside tests only"
)]

mod engine;
