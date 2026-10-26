//! The pawsman terminal UI. It owns the terminal; everything else lives in `pawsman-core`.
#![forbid(unsafe_code)]

mod app;
pub mod codes;
mod error;
pub mod input;
pub mod term;
mod views;

pub use app::SplashSetting;
pub use error::TuiError;
