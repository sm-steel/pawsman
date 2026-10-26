//! The pawsman terminal UI. It owns the terminal; everything else lives in `pawsman-core`.
#![forbid(unsafe_code)]

pub mod codes;
mod error;
pub mod input;
pub mod term;

pub use error::TuiError;
