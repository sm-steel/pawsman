//! Terminal ownership: modes, the capability probe and capabilities.

mod caps;
pub(crate) mod modes;
pub mod probe;
mod tmux;

pub use caps::{Caps, CapsHint, CellPx, ColorDepth, Graphics, KeyboardProtocol, OverSsh};
pub use tmux::{InTmux, passthrough};
