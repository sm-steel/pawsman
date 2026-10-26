//! The capability probe: query bytes, a pure parser for the replies (order does not matter), and
//! the reader that waits for them.

use std::io::{self, Read};
use std::os::fd::AsFd;
use std::time::{Duration, Instant};

use rustix::event::{PollFd, PollFlags, Timespec, poll};

use super::caps::CellPx;
use super::tmux::{InTmux, passthrough};

const GRAPHICS_QUERY: &[u8] = b"\x1b_Gi=31,s=1,v=1,a=q,t=d,f=24;AAAA\x1b\\";
/// Keyboard flags, cell size in pixels, and an SGR truecolour background read back by DECRQSS.
const PLAIN_QUERY: &[u8] = b"\x1b[?u\x1b[16t\x1b[48:2:1:2:3m\x1bP$qm\x1b\\\x1b[m";
const DA1: &[u8] = b"\x1b[c";

/// What to write and how many DA1 replies end the probe.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Query {
    bytes: Vec<u8>,
    da1_needed: usize,
}

impl Query {
    /// The bytes to write.
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// DA1 replies that end the probe.
    #[must_use]
    pub fn da1_needed(&self) -> usize {
        self.da1_needed
    }
}

/// The probe for this terminal. Inside tmux, tmux answers the plain DA1 itself, so the graphics
/// query and a second DA1 go through passthrough; the outer terminal answers them in order.
#[must_use]
pub fn query(tmux: InTmux) -> Query {
    match tmux {
        InTmux::No => Query {
            bytes: [GRAPHICS_QUERY, PLAIN_QUERY, DA1].concat(),
            da1_needed: 1,
        },
        InTmux::Yes => Query {
            bytes: [
                PLAIN_QUERY,
                DA1,
                &passthrough(GRAPHICS_QUERY),
                &passthrough(DA1),
            ]
            .concat(),
            da1_needed: 2,
        },
    }
}

/// One DA1 reply.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Da1 {
    /// Attribute 4: sixel graphics.
    pub sixel: bool,
}

/// Everything the replies said.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Replies {
    /// The kitty graphics query got `OK`.
    pub graphics: bool,
    /// `CSI ? flags u` arrived: the kitty keyboard protocol.
    pub keyboard: bool,
    /// `CSI 6 ; h ; w t`.
    pub cell_px: Option<CellPx>,
    /// DA1 replies in arrival order.
    pub da1: Vec<Da1>,
    /// DECRQSS read the truecolour background back.
    pub truecolor: bool,
}

/// Parses whatever replies arrived; unknown or truncated bytes are skipped.
#[must_use]
pub fn parse(bytes: &[u8]) -> Replies {
    let mut replies = Replies::default();
    let mut rest = bytes;
    while let Some(start) = rest.iter().position(|&b| b == 0x1b) {
        rest = rest.get(start..).unwrap_or_default();
        let consumed = match rest.get(1) {
            Some(b'_') => apc(rest, &mut replies),
            Some(b'P') => dcs(rest, &mut replies),
            Some(b'[') => csi(rest, &mut replies),
            _ => 1,
        };
        rest = rest.get(consumed.max(1)..).unwrap_or_default();
    }
    replies
}

/// The length of a string sequence up to and including `ESC \`, or all of `rest`.
fn string_end(rest: &[u8]) -> usize {
    find(rest, b"\x1b\\").map_or(rest.len(), |i| i + 2)
}

fn apc(rest: &[u8], replies: &mut Replies) -> usize {
    let end = string_end(rest);
    let body = rest.get(2..end.saturating_sub(2)).unwrap_or_default();
    if body.first() == Some(&b'G') && body.ends_with(b";OK") && find(body, b"i=31").is_some() {
        replies.graphics = true;
    }
    end
}

fn dcs(rest: &[u8], replies: &mut Replies) -> usize {
    let end = string_end(rest);
    let body = rest.get(2..end.saturating_sub(2)).unwrap_or_default();
    if body.starts_with(b"1$r")
        && (find(body, b"48:2:1:2:3").is_some() || find(body, b"48;2;1;2;3").is_some())
    {
        replies.truecolor = true;
    }
    end
}

fn csi(rest: &[u8], replies: &mut Replies) -> usize {
    let body = rest.get(2..).unwrap_or_default();
    let Some(fin) = body.iter().position(|b| (0x40..=0x7e).contains(b)) else {
        return rest.len();
    };
    let params = body.get(..fin).unwrap_or_default();
    match body.get(fin) {
        Some(b'u') if params.first() == Some(&b'?') => replies.keyboard = true,
        Some(b'c') if params.first() == Some(&b'?') => {
            let sixel = params
                .get(1..)
                .unwrap_or_default()
                .split(|&b| b == b';')
                .any(|a| a == b"4");
            replies.da1.push(Da1 { sixel });
        }
        Some(b't') => {
            let nums: Vec<u16> = std::str::from_utf8(params)
                .unwrap_or("")
                .split(';')
                .filter_map(|n| n.parse().ok())
                .collect();
            if let [6, h, w] = nums.as_slice() {
                replies.cell_px = Some(CellPx::new(*w, *h));
            }
        }
        _ => {}
    }
    2 + fin + 1
}

/// How long the probe waits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Timeouts {
    /// For all replies.
    pub reply: Duration,
    /// After the timeout, to swallow late replies.
    pub grace: Duration,
    /// Inside tmux, for the outer DA1 after tmux's own.
    pub tmux_grace: Duration,
}

impl Timeouts {
    /// 1 s, 150 ms, 500 ms (docs/research/kitty-graphics-over-ssh.md).
    pub const DEFAULT: Self = Self {
        reply: Duration::from_secs(1),
        grace: Duration::from_millis(150),
        tmux_grace: Duration::from_millis(500),
    };
}

/// What the probe read and how long it took.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Probed {
    /// Every reply byte.
    pub bytes: Vec<u8>,
    /// From the first read to the end.
    pub took: Duration,
}

/// Reads replies until `query.da1_needed()` DA1 replies arrived. On a timeout it keeps reading for
/// the grace period so a late reply does not reach the input; inside tmux, once tmux's own DA1 is
/// in, it waits at most `tmux_grace` for the outer one.
///
/// # Errors
///
/// Any I/O error from `poll` or `read`.
pub fn read_replies<F: Read + AsFd>(
    tty: &mut F,
    query: &Query,
    timeouts: Timeouts,
) -> io::Result<Probed> {
    let start = Instant::now();
    let mut bytes = Vec::new();
    let mut deadline = start + timeouts.reply;
    let mut in_grace = false;
    let mut short_wait = false;
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            if in_grace || short_wait {
                break;
            }
            in_grace = true;
            deadline = Instant::now() + timeouts.grace;
            continue;
        }
        if !poll_in(tty, left)? {
            continue;
        }
        let mut chunk = [0u8; 1024];
        let n = tty.read(&mut chunk)?;
        if n == 0 {
            break;
        }
        bytes.extend_from_slice(chunk.get(..n).unwrap_or_default());
        let seen = parse(&bytes).da1.len();
        if seen >= query.da1_needed() {
            break;
        }
        if seen >= 1 && query.da1_needed() > 1 && !short_wait {
            short_wait = true;
            deadline = deadline.min(Instant::now() + timeouts.tmux_grace);
        }
    }
    Ok(Probed {
        bytes,
        took: start.elapsed(),
    })
}

/// Waits up to `timeout` for `fd` to be readable.
pub(crate) fn poll_in<F: AsFd>(fd: &F, timeout: Duration) -> io::Result<bool> {
    let ts = Timespec {
        tv_sec: i64::try_from(timeout.as_secs()).unwrap_or(i64::MAX),
        tv_nsec: i64::from(timeout.subsec_nanos()),
    };
    let mut fds = [PollFd::new(fd, PollFlags::IN)];
    match poll(&mut fds, Some(&ts)) {
        Ok(n) => Ok(n > 0),
        Err(rustix::io::Errno::INTR) => Ok(false),
        Err(err) => Err(err.into()),
    }
}

pub(super) fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

#[cfg(test)]
mod tests {
    use std::io::Write as _;
    use std::os::unix::net::UnixStream;

    use super::*;

    #[test]
    fn outside_tmux_one_da1_ends_the_probe() {
        let q = query(InTmux::No);

        assert_eq!(q.da1_needed(), 1, "one DA1");
        assert!(q.bytes().ends_with(b"\x1b[c"), "DA1 last");
        assert!(
            q.bytes()
                .starts_with(b"\x1b_Gi=31,s=1,v=1,a=q,t=d,f=24;AAAA\x1b\\"),
            "graphics query first"
        );
    }

    #[test]
    fn inside_tmux_the_graphics_query_and_a_second_da1_pass_through() {
        let q = query(InTmux::Yes);

        assert_eq!(q.da1_needed(), 2, "tmux's DA1 plus the outer one");
        assert!(
            q.bytes().ends_with(b"\x1bPtmux;\x1b\x1b[c\x1b\\"),
            "outer DA1 through passthrough"
        );
        assert!(
            find(q.bytes(), b"\x1bPtmux;\x1b\x1b_G").is_some(),
            "graphics query through passthrough"
        );
    }

    #[test]
    fn kitty_answers_everything() {
        let r =
            parse(b"\x1b_Gi=31;OK\x1b\\\x1b[?1u\x1b[6;20;10t\x1bP1$r48:2:1:2:3m\x1b\\\x1b[?62;22c");

        assert!(
            r.graphics && r.keyboard && r.truecolor,
            "graphics, keyboard, truecolor: {r:?}"
        );
        assert_eq!(r.cell_px, Some(CellPx::new(10, 20)), "cell size");
        assert_eq!(r.da1, vec![Da1 { sixel: false }], "one DA1 without sixel");
    }

    #[test]
    fn graphics_error_is_not_support() {
        let r = parse(b"\x1b_Gi=31;ENOTSUPPORTED:no\x1b\\\x1b[?62c");

        assert!(!r.graphics, "only OK counts");
        assert_eq!(r.da1.len(), 1, "the DA1 still parses");
    }

    #[test]
    fn da1_replies_keep_their_order() {
        let r = parse(b"\x1b[?1;2;4c\x1b_Gi=31;OK\x1b\\\x1b[?62;22c");

        assert_eq!(
            r.da1,
            vec![Da1 { sixel: true }, Da1 { sixel: false }],
            "tmux first, outer second"
        );
    }

    #[test]
    fn decrqss_with_semicolons_counts_as_truecolor() {
        assert!(
            parse(b"\x1bP1$r48;2;1;2;3m\x1b\\").truecolor,
            "semicolon form"
        );
        assert!(!parse(b"\x1bP0$r\x1b\\").truecolor, "invalid request");
    }

    fn fast() -> Timeouts {
        Timeouts {
            reply: Duration::from_millis(300),
            grace: Duration::from_millis(50),
            tmux_grace: Duration::from_millis(120),
        }
    }

    #[test]
    fn reading_stops_at_the_da1() {
        let (mut ours, mut theirs) = UnixStream::pair().unwrap();
        theirs.write_all(b"\x1b_Gi=31;OK\x1b\\\x1b[?62c").unwrap();

        let probed = read_replies(&mut ours, &query(InTmux::No), fast()).unwrap();

        assert!(parse(&probed.bytes).graphics, "graphics reply read");
        assert!(
            probed.took < Duration::from_millis(100),
            "no waiting after DA1: {:?}",
            probed.took
        );
    }

    #[test]
    fn silence_times_out_after_reply_plus_grace_and_swallows_late_bytes() {
        let (mut ours, mut theirs) = UnixStream::pair().unwrap();
        let late = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(320));
            theirs.write_all(b"\x1b_Gi=31;OK\x1b\\").unwrap();
            theirs
        });

        let start = Instant::now();
        let probed = read_replies(&mut ours, &query(InTmux::No), fast()).unwrap();
        let _theirs = late.join().unwrap();

        assert!(
            start.elapsed() >= Duration::from_millis(340),
            "waited reply + grace: {:?}",
            start.elapsed()
        );
        assert!(
            parse(&probed.bytes).graphics,
            "the late reply landed in the probe, not the input"
        );
    }

    #[test]
    fn inside_tmux_a_missing_second_da1_costs_only_the_tmux_grace() {
        let (mut ours, mut theirs) = UnixStream::pair().unwrap();
        theirs.write_all(b"\x1b[?1;2;4c").unwrap();

        let probed = read_replies(&mut ours, &query(InTmux::Yes), fast()).unwrap();

        assert!(
            probed.took >= Duration::from_millis(110),
            "waited for the outer DA1: {:?}",
            probed.took
        );
        assert!(
            probed.took < Duration::from_millis(250),
            "but not the full timeout: {:?}",
            probed.took
        );
    }

    #[test]
    fn inside_tmux_both_da1_end_it_at_once() {
        let (mut ours, mut theirs) = UnixStream::pair().unwrap();
        theirs
            .write_all(b"\x1b[?1;2;4c\x1b_Gi=31;OK\x1b\\\x1b[?62;22c")
            .unwrap();

        let probed = read_replies(&mut ours, &query(InTmux::Yes), fast()).unwrap();

        assert!(
            probed.took < Duration::from_millis(100),
            "outer DA1 ends it: {:?}",
            probed.took
        );
        assert_eq!(parse(&probed.bytes).da1.len(), 2, "both DA1 read");
    }

    #[test]
    fn garbage_and_truncation_never_panic() {
        for bytes in [
            &b"\x1b"[..],
            b"\x1b[",
            b"\x1b_G",
            b"\x1b[6;x;yt",
            b"\x1bP1$r",
            b"\xff\xfe",
        ] {
            let _ = parse(bytes);
        }
    }
}
