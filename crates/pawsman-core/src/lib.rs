//! The pawsman engine. It has no terminal dependencies: the TUI and the CLI are thin over it.
#![forbid(unsafe_code)]

pub mod codes;
pub mod engine;
pub mod env;
mod error;
pub mod globals;
pub mod keys;
pub mod notice;
pub mod paths;
pub mod settings;
pub mod state;
pub mod store;
pub mod wire;

pub use error::CoreError;
