//! The capability probe: query bytes, and a pure parser for the replies (order does not matter).

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
    let _ = (tmux, GRAPHICS_QUERY, PLAIN_QUERY, DA1, passthrough);
    Query {
        bytes: Vec::new(),
        da1_needed: 0,
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
    let _ = bytes;
    Replies::default()
}

pub(super) fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

#[cfg(test)]
mod tests {
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
