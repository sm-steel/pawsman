//! Keyboard, mouse, paste and focus input, decoded by pawsman itself from `/dev/tty`.

mod decoder;
mod reader;

use std::time::Duration;

pub use decoder::{Button, Decoder, Input, Mouse, MouseKind};
pub(crate) use reader::Reader;

/// How long a lone `ESC` waits for the rest of a sequence before it counts as the Esc key.
pub const ESC_TIMEOUT: Duration = Duration::from_millis(25);
/// How long an open bracketed paste waits for its end marker before it is closed, so a lost
/// `ESC [ 201 ~` never swallows later keys.
pub const PASTE_TIMEOUT: Duration = Duration::from_secs(1);
/// The longest escape sequence or string kept; a longer one is dropped.
pub const MAX_SEQUENCE: usize = 64 * 1024;
/// The largest paste kept; the rest of a longer paste is dropped.
pub const MAX_PASTE: usize = 8 * 1024 * 1024;
