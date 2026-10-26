//! The main loop and its state.

use crate::input::Input;

/// Everything the main loop reacts to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Msg {
    /// Decoded input.
    Input(Input),
    /// The terminal was resized.
    Resize,
    /// SIGTERM, SIGHUP or SIGINT.
    Terminate,
    /// The input thread hit end of file or an error.
    InputClosed,
}
