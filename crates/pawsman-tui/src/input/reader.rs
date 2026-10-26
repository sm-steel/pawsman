//! The input thread: polls the tty and the signal pipes, decodes bytes and sends `Msg`s.

use std::io::{self, Read};
use std::os::fd::AsFd;
use std::os::unix::net::UnixStream;
use std::sync::mpsc::Sender;
use std::thread::JoinHandle;

use rustix::event::{PollFd, PollFlags, Timespec, poll};
use signal_hook::SigId;
use signal_hook::consts::{SIGHUP, SIGINT, SIGTERM, SIGWINCH};

use tracing::{debug, warn};

use super::{Decoder, Input};
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

/// The thread body. It owns the sender, so the loop sees the channel close when the thread ends.
#[expect(
    clippy::needless_pass_by_value,
    reason = "the thread owns its stop pipe and sender; dropping the sender tells the loop it ended"
)]
pub(crate) fn run(
    mut tty: impl Read + AsFd,
    mut winch: UnixStream,
    mut term: UnixStream,
    stop: UnixStream,
    tx: Sender<Msg>,
) {
    let mut decoder = Decoder::new();
    let mut buf = [0u8; 4096];
    loop {
        let timeout = decoder.has_pending().then(|| {
            let silence = decoder.silence();
            Timespec {
                tv_sec: i64::try_from(silence.as_secs()).unwrap_or(i64::MAX),
                tv_nsec: i64::from(silence.subsec_nanos()),
            }
        });
        let ready = {
            let mut fds = [
                PollFd::new(&tty, PollFlags::IN),
                PollFd::new(&winch, PollFlags::IN),
                PollFd::new(&term, PollFlags::IN),
                PollFd::new(&stop, PollFlags::IN),
            ];
            match poll(&mut fds, timeout.as_ref()) {
                Ok(0) => None,
                Ok(_) => Some(fds.map(|f| !f.revents().is_empty())),
                Err(rustix::io::Errno::INTR) => continue,
                Err(err) => {
                    warn!(%err, "input poll");
                    let _ = tx.send(Msg::InputClosed);
                    return;
                }
            }
        };
        let Some([tty_ready, winch_ready, term_ready, stop_ready]) = ready else {
            if !send_all(&tx, decoder.flush()) {
                return;
            }
            continue;
        };
        if stop_ready {
            debug!("input thread stopped");
            return;
        }
        if term_ready {
            drain(&mut term);
            let _ = tx.send(Msg::Terminate);
            return;
        }
        if winch_ready {
            drain(&mut winch);
            if tx.send(Msg::Resize).is_err() {
                return;
            }
        }
        if tty_ready {
            match tty.read(&mut buf) {
                Ok(0) | Err(_) => {
                    let _ = tx.send(Msg::InputClosed);
                    return;
                }
                Ok(n) => {
                    if !send_all(&tx, decoder.feed(buf.get(..n).unwrap_or_default())) {
                        return;
                    }
                }
            }
        }
    }
}

/// Sends every input; false once the loop has gone.
fn send_all(tx: &Sender<Msg>, inputs: Vec<Input>) -> bool {
    inputs
        .into_iter()
        .all(|input| tx.send(Msg::Input(input)).is_ok())
}

fn drain(pipe: &mut UnixStream) {
    let mut sink = [0u8; 64];
    let _ = pipe.read(&mut sink);
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
    fn a_stalled_paste_lets_ctrl_q_through() {
        let mut rig = rig();

        rig.tty.write_all(b"\x1b[200~ab").unwrap();
        let paste = rig.rx.recv_timeout(Duration::from_secs(3));
        rig.tty.write_all(b"\x11").unwrap();
        let quit = next(&rig.rx);

        assert_eq!(
            paste,
            Ok(Msg::Input(Input::Paste("ab".to_owned()))),
            "paste closed by silence"
        );
        assert_eq!(
            quit,
            Msg::Input(Input::Key(Chord::ctrl('q'))),
            "Ctrl-Q arrives"
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
