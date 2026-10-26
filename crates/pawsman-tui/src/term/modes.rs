//! Every terminal mode pawsman turns on, and the one place that undoes them, newest first: on exit,
//! on error, from `Drop` and from the panic hook (rules/tui.md §2).

use std::io::Write;
use std::os::fd::OwnedFd;
use std::sync::{Mutex, PoisonError, TryLockError};

use rustix::termios::{OptionalActions, Termios, tcsetattr};

/// One terminal mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Raw input through termios; undone by restoring the saved termios.
    Raw,
    /// The alternate screen.
    AltScreen,
    /// The cursor is hidden.
    HiddenCursor,
    /// Kitty keyboard protocol, disambiguate flag only.
    KittyKeyboard,
    /// Bracketed paste.
    BracketedPaste,
    /// Focus events.
    Focus,
    /// Button-event mouse tracking with SGR coordinates; never any-motion (1003).
    Mouse,
}

/// The modes in setup order.
pub const MODES: &[Mode] = &[
    Mode::Raw,
    Mode::AltScreen,
    Mode::HiddenCursor,
    Mode::KittyKeyboard,
    Mode::BracketedPaste,
    Mode::Focus,
    Mode::Mouse,
];

impl Mode {
    /// The bytes that turn the mode on; empty for `Raw`.
    #[must_use]
    pub const fn on(self) -> &'static [u8] {
        match self {
            Self::Raw => b"",
            Self::AltScreen => b"\x1b[?1049h",
            Self::HiddenCursor => b"\x1b[?25l",
            Self::KittyKeyboard => b"\x1b[>1u",
            Self::BracketedPaste => b"\x1b[?2004h",
            Self::Focus => b"\x1b[?1004h",
            Self::Mouse => b"\x1b[?1000h\x1b[?1002h\x1b[?1006h",
        }
    }

    /// The bytes that turn it off; empty for `Raw`.
    #[must_use]
    pub const fn off(self) -> &'static [u8] {
        match self {
            Self::Raw => b"",
            Self::AltScreen => b"\x1b[?1049l",
            Self::HiddenCursor => b"\x1b[?25h",
            Self::KittyKeyboard => b"\x1b[<u",
            Self::BracketedPaste => b"\x1b[?2004l",
            Self::Focus => b"\x1b[?1004l",
            Self::Mouse => b"\x1b[?1006l\x1b[?1002l\x1b[?1000l",
        }
    }

    /// A name for errors and logs.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Raw => "raw",
            Self::AltScreen => "alternate screen",
            Self::HiddenCursor => "hidden cursor",
            Self::KittyKeyboard => "kitty keyboard",
            Self::BracketedPaste => "bracketed paste",
            Self::Focus => "focus events",
            Self::Mouse => "mouse",
        }
    }
}

/// Active modes, the terminal to write their undo to, and the termios to restore.
pub struct Stack {
    out: Box<dyn Write + Send>,
    raw: Option<(OwnedFd, Termios)>,
    active: Vec<Mode>,
}

impl Stack {
    /// A stack writing undo sequences to `out`.
    #[must_use]
    pub fn new(out: Box<dyn Write + Send>) -> Self {
        Self {
            out,
            raw: None,
            active: Vec::new(),
        }
    }

    /// Remembers the termios to restore when `Raw` is undone.
    #[must_use]
    pub fn with_raw(mut self, fd: OwnedFd, saved: Termios) -> Self {
        self.raw = Some((fd, saved));
        self
    }

    /// Records that `mode` is on.
    pub fn push(&mut self, mode: Mode) {
        self.active.push(mode);
    }

    /// The modes that are on, oldest first.
    #[must_use]
    pub fn active(&self) -> &[Mode] {
        &self.active
    }

    /// Undoes every active mode, newest first; errors are ignored, since this runs on the way out.
    pub fn undo_all(&mut self) {
        while let Some(mode) = self.active.pop() {
            if mode == Mode::Raw {
                if let Some((fd, saved)) = &self.raw {
                    let _ = tcsetattr(fd, OptionalActions::Now, saved);
                }
            } else {
                let _ = self.out.write_all(mode.off());
            }
        }
        let _ = self.out.flush();
    }
}

static STACK: Mutex<Option<Stack>> = Mutex::new(None);

/// Makes `stack` the process-wide stack that `restore` and the panic hook undo.
pub fn install(stack: Stack) {
    *STACK.lock().unwrap_or_else(PoisonError::into_inner) = Some(stack);
}

/// Records `mode` on the process-wide stack.
pub fn push(mode: Mode) {
    if let Some(stack) = STACK
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .as_mut()
    {
        stack.push(mode);
    }
}

/// Undoes every mode. Safe to call more than once.
pub fn restore() {
    if let Some(stack) = STACK
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .as_mut()
    {
        stack.undo_all();
    }
}

/// Like `restore`, but never blocks: a panic while the stack is locked leaves it as it is.
pub fn restore_from_panic() {
    let mut guard = match STACK.try_lock() {
        Ok(guard) => guard,
        Err(TryLockError::Poisoned(poisoned)) => poisoned.into_inner(),
        Err(TryLockError::WouldBlock) => return,
    };
    if let Some(stack) = guard.as_mut() {
        stack.undo_all();
    }
}

/// Restores the terminal before the default panic message, on the main thread only: a worker's
/// panic ends its thread, and the main loop sees the closed channel and exits cleanly.
pub fn install_panic_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        tracing::error!(panic = %info, "panic");
        if std::thread::current().name() == Some("main") {
            restore_from_panic();
        }
        previous(info);
    }));
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::*;

    #[derive(Clone, Default)]
    struct Shared(Arc<Mutex<Vec<u8>>>);

    impl Write for Shared {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn modes_are_undone_newest_first() {
        let out = Shared::default();
        let mut stack = Stack::new(Box::new(out.clone()));
        for mode in [
            Mode::AltScreen,
            Mode::HiddenCursor,
            Mode::KittyKeyboard,
            Mode::BracketedPaste,
            Mode::Focus,
            Mode::Mouse,
        ] {
            stack.push(mode);
        }

        stack.undo_all();

        let bytes = out.0.lock().unwrap().clone();
        assert_eq!(
            bytes,
            b"\x1b[?1006l\x1b[?1002l\x1b[?1000l\x1b[?1004l\x1b[?2004l\x1b[<u\x1b[?25h\x1b[?1049l"
                .to_vec(),
            "mouse, focus, paste, keyboard, cursor, alternate screen"
        );
        assert!(stack.active().is_empty(), "nothing left");
    }

    #[test]
    fn undo_twice_writes_once() {
        let out = Shared::default();
        let mut stack = Stack::new(Box::new(out.clone()));
        stack.push(Mode::AltScreen);

        stack.undo_all();
        stack.undo_all();

        assert_eq!(
            out.0.lock().unwrap().as_slice(),
            b"\x1b[?1049l",
            "idempotent"
        );
    }

    #[test]
    fn mouse_never_enables_any_motion() {
        assert!(
            !Mode::Mouse.on().windows(4).any(|w| w == b"1003"),
            "no any-motion tracking"
        );
        assert_eq!(Mode::KittyKeyboard.on(), b"\x1b[>1u", "disambiguate only");
    }

    #[test]
    fn every_mode_has_an_undo() {
        for mode in MODES {
            assert!(
                *mode == Mode::Raw || !mode.off().is_empty(),
                "{mode:?} has an off sequence"
            );
        }
    }
}
