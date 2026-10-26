//! The pawsman terminal UI. It owns the terminal; everything else lives in `pawsman-core`.
#![forbid(unsafe_code)]

mod app;
pub mod codes;
mod error;
pub mod input;
mod options;
pub mod term;
mod views;

pub use app::{SplashSetting, run};
pub use error::TuiError;
pub use options::Options;
