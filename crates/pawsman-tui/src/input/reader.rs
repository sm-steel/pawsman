//! The input thread: polls the tty and the signal pipes, decodes bytes and sends `Msg`s.

use std::io::{self, Read};
use std::os::fd::AsFd;
use std::os::unix::net::UnixStream;
use std::sync::mpsc::Sender;
use std::thread::JoinHandle;

use signal_hook::SigId;
use signal_hook::consts::{SIGHUP, SIGINT, SIGTERM, SIGWINCH};

use crate::app::Msg;

/// The running input thread.
pub(crate) struct Reader {
    handle: JoinHandle<()>,
    stop: UnixStream,
    signals: Vec<SigId>,
}

impl Reader {
    /// Starts the thread on `tty`, sending to `tx`.
    pub(crate) fn spawn(tty: std::fs::File, tx: Sender<Msg>) -> io::Result<Self> {
        let (winch_w, winch_r) = UnixStream::pair()?;
        let (term_w, term_r) = UnixStream::pair()?;
        let (stop, stop_r) = UnixStream::pair()?;
        let mut signals = vec![signal_hook::low_level::pipe::register(SIGWINCH, winch_w)?];
        for signal in [SIGTERM, SIGHUP, SIGINT] {
            signals.push(signal_hook::low_level::pipe::register(
                signal,
                term_w.try_clone()?,
            )?);
        }
        let handle = std::thread::Builder::new()
            .name("pawsman-input".into())
            .spawn(move || run(tty, winch_r, term_r, stop_r, tx))?;
        Ok(Self {
            handle,
            stop,
            signals,
        })
    }

    /// Stops and joins the thread and unregisters the signal handlers.
    pub(crate) fn stop(mut self) {
        use std::io::Write as _;
        let _ = self.stop.write_all(b"x");
        let _ = self.handle.join();
        for id in self.signals {
            signal_hook::low_level::unregister(id);
        }
    }
}

/// The thread body.
pub(crate) fn run(
    tty: impl Read + AsFd,
    winch: UnixStream,
    term: UnixStream,
    stop: UnixStream,
    tx: Sender<Msg>,
) {
    let _ = (tty, winch, term, stop, tx);
}

#[cfg(test)]
mod tests {
    use std::io::Write as _;
    use std::os::unix::net::UnixStream;
    use std::sync::mpsc;
    use std::time::Duration;

    use pawsman_core::keys::{Chord, KeyCode};

    use super::*;
    use crate::input::Input;

    struct Rig {
        tty: UnixStream,
        winch: UnixStream,
        term: UnixStream,
        stop: UnixStream,
        rx: mpsc::Receiver<Msg>,
        handle: std::thread::JoinHandle<()>,
    }

    fn rig() -> Rig {
        let (tty, tty_in) = UnixStream::pair().unwrap();
        let (winch, winch_in) = UnixStream::pair().unwrap();
        let (term, term_in) = UnixStream::pair().unwrap();
        let (stop, stop_in) = UnixStream::pair().unwrap();
        let (tx, rx) = mpsc::channel();
        let handle = std::thread::spawn(move || run(tty_in, winch_in, term_in, stop_in, tx));
        Rig {
            tty,
            winch,
            term,
            stop,
            rx,
            handle,
        }
    }

    fn next(rx: &mpsc::Receiver<Msg>) -> Msg {
        rx.recv_timeout(Duration::from_secs(1))
            .expect("a message within a second")
    }

    #[test]
    fn keys_arrive_and_a_lone_esc_flushes_after_the_timeout() {
        let mut rig = rig();

        rig.tty.write_all(b"a\x1b").unwrap();

        assert_eq!(
            next(&rig.rx),
            Msg::Input(Input::Key(Chord::plain(KeyCode::Char('a')))),
            "a"
        );
        assert_eq!(
            next(&rig.rx),
            Msg::Input(Input::Key(Chord::plain(KeyCode::Esc))),
            "Esc after the timeout"
        );
        rig.stop.write_all(b"x").unwrap();
        rig.handle.join().unwrap();
    }

    #[test]
    fn signals_become_messages() {
        let mut rig = rig();

        rig.winch.write_all(b"w").unwrap();
        let resize = next(&rig.rx);
        rig.term.write_all(b"t").unwrap();
        let terminate = next(&rig.rx);

        assert_eq!(resize, Msg::Resize, "SIGWINCH");
        assert_eq!(terminate, Msg::Terminate, "SIGTERM, SIGHUP, SIGINT");
        rig.handle.join().unwrap();
    }

    #[test]
    fn a_closed_tty_reports_input_closed() {
        let rig = rig();

        drop(rig.tty);

        assert_eq!(next(&rig.rx), Msg::InputClosed, "end of input");
        rig.handle.join().unwrap();
    }

    #[test]
    fn stop_ends_the_thread_quietly() {
        let mut rig = rig();

        rig.stop.write_all(b"x").unwrap();
        rig.handle.join().unwrap();

        assert!(rig.rx.try_recv().is_err(), "no message on stop");
    }
}
