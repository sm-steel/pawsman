//! Terminal capabilities, resolved once from the probe replies and the env snapshot. Views branch
//! on these, never on `$TERM`.

use pawsman_core::env::EnvSnapshot;
use pawsman_core::settings::ImageMode;

use super::probe::Replies;
use super::tmux::InTmux;

/// How images are drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Graphics {
    /// Kitty graphics protocol through Unicode placeholders.
    Kitty,
    /// Sixel.
    Sixel,
    /// Half blocks.
    Blocks,
    /// A text placeholder.
    Off,
}

/// Which key encoding the terminal speaks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyboardProtocol {
    /// The kitty keyboard protocol (disambiguate flag).
    Kitty,
    /// Legacy escape sequences.
    Legacy,
}

/// Colours the terminal shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorDepth {
    /// 24-bit RGB.
    TrueColor,
    /// The xterm 256-colour palette.
    Indexed256,
    /// The 16 ANSI colours.
    Ansi16,
}

/// Whether the session runs over SSH.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverSsh {
    /// `SSH_TTY` or `SSH_CONNECTION` is set.
    Yes,
    /// Local.
    No,
}

/// A cell's size in pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CellPx {
    width: u16,
    height: u16,
}

impl CellPx {
    /// A cell size.
    #[must_use]
    pub const fn new(width: u16, height: u16) -> Self {
        Self { width, height }
    }

    /// Width in pixels.
    #[must_use]
    pub const fn width(self) -> u16 {
        self.width
    }

    /// Height in pixels.
    #[must_use]
    pub const fn height(self) -> u16 {
        self.height
    }
}

/// Something the user can change to get more.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CapsHint {
    /// tmux without a UTF-8 locale rewrites image placeholders and half blocks.
    TmuxNeedsUtf8,
    /// tmux did not pass the graphics query through.
    TmuxPassthrough,
}

impl CapsHint {
    /// The text the hint bar shows.
    #[must_use]
    pub fn text(self) -> &'static str {
        match self {
            Self::TmuxNeedsUtf8 => "images off: run tmux with -u",
            Self::TmuxPassthrough => "images: set allow-passthrough on in tmux",
        }
    }
}

/// The resolved capabilities.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Caps {
    graphics: Graphics,
    keyboard: KeyboardProtocol,
    color: ColorDepth,
    cell_px: Option<CellPx>,
    tmux: InTmux,
    ssh: OverSsh,
    hint: Option<CapsHint>,
}

impl Caps {
    /// Resolves the capabilities; an explicit `images` mode wins over the probe.
    #[must_use]
    pub fn resolve(replies: &Replies, env: &EnvSnapshot, images: ImageMode) -> Self {
        let tmux = InTmux::from_env(env);
        let dumb = env.term() == Some("dumb");
        let (graphics, hint) = graphics(replies, env, images, tmux);
        let color = if dumb {
            ColorDepth::Ansi16
        } else if env.truecolor() || replies.truecolor {
            ColorDepth::TrueColor
        } else if tmux == InTmux::Yes || env.term().is_some_and(|t| t.contains("256")) {
            ColorDepth::Indexed256
        } else {
            ColorDepth::Ansi16
        };
        Self {
            graphics,
            keyboard: if replies.keyboard {
                KeyboardProtocol::Kitty
            } else {
                KeyboardProtocol::Legacy
            },
            color,
            cell_px: replies.cell_px,
            tmux,
            ssh: if env.over_ssh() {
                OverSsh::Yes
            } else {
                OverSsh::No
            },
            hint,
        }
    }

    /// Image mode.
    #[must_use]
    pub fn graphics(&self) -> Graphics {
        self.graphics
    }

    /// Key encoding.
    #[must_use]
    pub fn keyboard(&self) -> KeyboardProtocol {
        self.keyboard
    }

    /// Colour depth.
    #[must_use]
    pub fn color(&self) -> ColorDepth {
        self.color
    }

    /// Cell size, when the terminal reported it.
    #[must_use]
    pub fn cell_px(&self) -> Option<CellPx> {
        self.cell_px
    }

    /// Inside tmux.
    #[must_use]
    pub fn tmux(&self) -> InTmux {
        self.tmux
    }

    /// Over SSH.
    #[must_use]
    pub fn ssh(&self) -> OverSsh {
        self.ssh
    }

    /// A hint for the hint bar.
    #[must_use]
    pub fn hint(&self) -> Option<CapsHint> {
        self.hint
    }
}

/// The image mode and any hint. Inside tmux only the outer terminal's DA1 (the second) decides
/// sixel, since tmux answers the first itself and claims sixel on the outer terminal's behalf.
fn graphics(
    replies: &Replies,
    env: &EnvSnapshot,
    images: ImageMode,
    tmux: InTmux,
) -> (Graphics, Option<CapsHint>) {
    let utf8 = env.utf8_locale();
    let dumb = env.term() == Some("dumb");
    let outer_da1 = match tmux {
        InTmux::Yes => replies.da1.get(1),
        InTmux::No => replies.da1.first(),
    };
    match images {
        ImageMode::Kitty => (Graphics::Kitty, None),
        ImageMode::Sixel => (Graphics::Sixel, None),
        ImageMode::Blocks => (Graphics::Blocks, None),
        ImageMode::Off => (Graphics::Off, None),
        ImageMode::Auto if dumb => (Graphics::Off, None),
        ImageMode::Auto if tmux == InTmux::Yes && !utf8 => {
            (Graphics::Off, Some(CapsHint::TmuxNeedsUtf8))
        }
        ImageMode::Auto if replies.graphics => (Graphics::Kitty, None),
        ImageMode::Auto => {
            let hint = (tmux == InTmux::Yes).then_some(CapsHint::TmuxPassthrough);
            let fallback = if outer_da1.is_some_and(|d| d.sixel) {
                Graphics::Sixel
            } else if utf8 {
                Graphics::Blocks
            } else {
                Graphics::Off
            };
            (fallback, hint)
        }
    }
}

#[cfg(test)]
mod tests {
    use pawsman_core::env::EnvSnapshot;
    use pawsman_core::settings::ImageMode;
    use rstest::rstest;

    use super::*;
    use crate::term::probe::parse;

    const KITTY: &[u8] = b"\x1b_Gi=31;OK\x1b\\\x1b[?1u\x1b[6;20;10t\x1b[?62;22c";
    const TMUX_PASSTHROUGH: &[u8] = b"\x1b[?1;2;4c\x1b_Gi=31;OK\x1b\\\x1b[?62;22c";
    const TMUX_NO_PASSTHROUGH: &[u8] = b"\x1b[?1;2;4c";
    const XTERM: &[u8] = b"\x1b[?63;1;2;4;6;9;15;22c";
    const UTF8: (&str, &str) = ("LANG", "C.UTF-8");
    const TMUX: (&str, &str) = ("TMUX", "/tmp/tmux-1000/default,1,0");

    fn env(pairs: &[(&str, &str)]) -> EnvSnapshot {
        EnvSnapshot::from_pairs(pairs.iter().copied())
    }

    #[rstest]
    #[case::kitty(KITTY, &[UTF8, ("TERM", "xterm-kitty")], Graphics::Kitty, None)]
    #[case::tmux_passthrough(TMUX_PASSTHROUGH, &[UTF8, TMUX], Graphics::Kitty, None)]
    #[case::tmux_sixel_claim_ignored(TMUX_NO_PASSTHROUGH, &[UTF8, TMUX], Graphics::Blocks, Some(CapsHint::TmuxPassthrough))]
    #[case::tmux_without_utf8(TMUX_PASSTHROUGH, &[("LANG", "C"), TMUX], Graphics::Off, Some(CapsHint::TmuxNeedsUtf8))]
    #[case::xterm_sixel(XTERM, &[UTF8, ("TERM", "xterm")], Graphics::Sixel, None)]
    #[case::silence_utf8(b"", &[UTF8], Graphics::Blocks, None)]
    #[case::silence_no_utf8(b"", &[], Graphics::Off, None)]
    #[case::dumb(KITTY, &[UTF8, ("TERM", "dumb")], Graphics::Off, None)]
    fn auto_graphics_follow_the_probe(
        #[case] replies: &[u8],
        #[case] pairs: &[(&str, &str)],
        #[case] graphics: Graphics,
        #[case] hint: Option<CapsHint>,
    ) {
        let caps = Caps::resolve(&parse(replies), &env(pairs), ImageMode::Auto);

        assert_eq!(caps.graphics(), graphics, "graphics for {pairs:?}");
        assert_eq!(caps.hint(), hint, "hint for {pairs:?}");
    }

    #[test]
    fn an_explicit_mode_wins() {
        let caps = Caps::resolve(&parse(b""), &env(&[]), ImageMode::Kitty);

        assert_eq!(caps.graphics(), Graphics::Kitty, "forced kitty");
    }

    #[rstest]
    #[case::colorterm(&[("COLORTERM", "truecolor"), ("TERM", "xterm")], b"", ColorDepth::TrueColor)]
    #[case::decrqss(&[("TERM", "xterm")], b"\x1bP1$r48:2:1:2:3m\x1b\\", ColorDepth::TrueColor)]
    #[case::term_256(&[("TERM", "xterm-256color")], b"", ColorDepth::Indexed256)]
    #[case::tmux(&[TMUX, ("TERM", "screen")], b"", ColorDepth::Indexed256)]
    #[case::plain(&[("TERM", "xterm")], b"", ColorDepth::Ansi16)]
    #[case::dumb(&[("TERM", "dumb"), ("COLORTERM", "truecolor")], b"", ColorDepth::Ansi16)]
    fn color_depth(
        #[case] pairs: &[(&str, &str)],
        #[case] replies: &[u8],
        #[case] expected: ColorDepth,
    ) {
        let caps = Caps::resolve(&parse(replies), &env(pairs), ImageMode::Auto);

        assert_eq!(caps.color(), expected, "colour for {pairs:?}");
    }

    #[test]
    fn keyboard_comes_from_the_probe_only() {
        let kitty = Caps::resolve(&parse(KITTY), &env(&[]), ImageMode::Auto);
        let tmux = Caps::resolve(&parse(TMUX_PASSTHROUGH), &env(&[TMUX]), ImageMode::Auto);

        assert_eq!(
            kitty.keyboard(),
            KeyboardProtocol::Kitty,
            "kitty answers CSI ? u"
        );
        assert_eq!(tmux.keyboard(), KeyboardProtocol::Legacy, "tmux does not");
    }

    #[test]
    fn tmux_and_ssh_come_from_the_env() {
        let caps = Caps::resolve(
            &parse(b""),
            &env(&[TMUX, ("SSH_TTY", "/dev/pts/1")]),
            ImageMode::Auto,
        );

        assert_eq!(caps.tmux(), InTmux::Yes, "tmux");
        assert_eq!(caps.ssh(), OverSsh::Yes, "ssh");
    }
}
