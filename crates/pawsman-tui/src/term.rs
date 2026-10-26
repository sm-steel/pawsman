//! Terminal ownership: modes, the capability probe and capabilities.

mod caps;
pub(crate) mod modes;
pub mod probe;
mod tmux;

use std::fs::{File, OpenOptions};
use std::io::Write as _;
use std::os::fd::AsFd as _;
use std::os::unix::ffi::OsStringExt as _;
use std::path::PathBuf;

pub use caps::{Caps, CapsHint, CellPx, ColorDepth, Graphics, KeyboardProtocol, OverSsh};
use modes::{Mode, Stack};
use probe::{Probed, Timeouts};
use rustix::termios::{OptionalActions, tcgetattr, tcsetattr};
pub use tmux::{InTmux, passthrough};

use crate::TuiError;

/// The terminal, owned by the main loop.
pub struct Term {
    tty: File,
}

impl Term {
    /// Opens `/dev/tty`, enters raw mode and installs the panic hook; the probe comes next, then
    /// [`enter_screen`](Self::enter_screen).
    ///
    /// # Errors
    ///
    /// `tui::no_tty` when there is no terminal; `tui::terminal_mode` when raw mode cannot be set.
    pub fn open() -> Result<Self, TuiError> {
        let tty = OpenOptions::new()
            .read(true)
            .write(true)
            .open(device_path())
            .map_err(TuiError::no_tty)?;
        let saved = tcgetattr(&tty).map_err(|e| TuiError::terminal_mode(Mode::Raw, e.into()))?;
        let fd = tty.as_fd().try_clone_to_owned().map_err(TuiError::no_tty)?;
        let out = tty.try_clone().map_err(TuiError::no_tty)?;
        modes::install(Stack::new(Box::new(out)).with_raw(fd, saved.clone()));
        modes::install_panic_hook();
        let mut raw = saved;
        raw.make_raw();
        tcsetattr(&tty, OptionalActions::Now, &raw)
            .map_err(|e| TuiError::terminal_mode(Mode::Raw, e.into()))?;
        modes::push(Mode::Raw);
        Ok(Self { tty })
    }

    /// Enters the alternate screen and hides the cursor. Runs after the probe: inside tmux the
    /// passthrough replies did not arrive when the probe followed the alternate-screen switch.
    ///
    /// # Errors
    ///
    /// `tui::terminal_mode` when the alternate screen cannot be set; the modes already set are
    /// undone first.
    pub fn enter_screen(&mut self) -> Result<(), TuiError> {
        for mode in [Mode::AltScreen, Mode::HiddenCursor] {
            if let Err(err) = self.enable(mode) {
                modes::restore();
                return Err(err);
            }
        }
        Ok(())
    }

    fn enable(&mut self, mode: Mode) -> Result<(), TuiError> {
        self.tty
            .write_all(mode.on())
            .and_then(|()| self.tty.flush())
            .map_err(|e| TuiError::terminal_mode(mode, e))?;
        modes::push(mode);
        Ok(())
    }

    /// Writes the probe and reads the replies.
    ///
    /// # Errors
    ///
    /// `tui::terminal_io` on a write or read failure.
    pub fn probe(&mut self, tmux: InTmux) -> Result<Probed, TuiError> {
        let query = probe::query(tmux);
        self.tty
            .write_all(query.bytes())
            .and_then(|()| self.tty.flush())
            .map_err(TuiError::terminal_io)?;
        probe::read_replies(&mut self.tty, &query, Timeouts::DEFAULT).map_err(TuiError::terminal_io)
    }

    /// Turns on the input modes: kitty keyboard flags when the probe found them, bracketed paste,
    /// focus events and mouse buttons. A mode that fails is logged and skipped.
    pub fn enable_input(&mut self, keyboard: KeyboardProtocol) {
        let kitty = (keyboard == KeyboardProtocol::Kitty).then_some(Mode::KittyKeyboard);
        for mode in kitty
            .into_iter()
            .chain([Mode::BracketedPaste, Mode::Focus, Mode::Mouse])
        {
            if let Err(err) = self.enable(mode) {
                tracing::warn!(mode = mode.name(), %err, "optional terminal mode skipped");
            }
        }
    }

    /// A handle for the input thread.
    ///
    /// # Errors
    ///
    /// `tui::terminal_io` when the descriptor cannot be duplicated.
    pub fn reader(&self) -> Result<File, TuiError> {
        self.tty.try_clone().map_err(TuiError::terminal_io)
    }

    /// A handle for the ratatui backend.
    ///
    /// # Errors
    ///
    /// `tui::terminal_io` when the descriptor cannot be duplicated.
    pub fn writer(&self) -> Result<File, TuiError> {
        self.tty.try_clone().map_err(TuiError::terminal_io)
    }

    /// Undoes every mode, newest first.
    pub fn restore(&mut self) {
        modes::restore();
    }
}

/// The terminal device: the real name of the first standard stream that is a terminal, else
/// `/dev/tty`. macOS cannot `poll` the `/dev/tty` alias, only the device behind it.
fn device_path() -> PathBuf {
    let stdin = std::io::stdin();
    [
        stdin.as_fd(),
        rustix::stdio::stdout(),
        rustix::stdio::stderr(),
    ]
    .into_iter()
    .filter(|fd| rustix::termios::isatty(fd))
    .find_map(|fd| rustix::termios::ttyname(fd, Vec::new()).ok())
    .map_or_else(
        || PathBuf::from("/dev/tty"),
        |name| PathBuf::from(std::ffi::OsString::from_vec(name.into_bytes())),
    )
}

impl Drop for Term {
    fn drop(&mut self) {
        modes::restore();
    }
}
