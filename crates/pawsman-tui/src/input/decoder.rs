//! A pure, stateful decoder from terminal bytes to inputs: legacy keys, kitty `CSI … u` under the
//! disambiguate flag, SGR mouse, bracketed paste and focus. Terminal replies (APC, DCS, OSC, and the
//! probe's CSI replies) are dropped, so a late probe reply never types anything.

use pawsman_core::keys::{Chord, Mods};

/// One decoded input.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Input {
    /// A key press or repeat.
    Key(Chord),
    /// Text from bracketed paste.
    Paste(String),
    /// A mouse event.
    Mouse(Mouse),
    /// The terminal window gained focus.
    FocusGained,
    /// The terminal window lost focus.
    FocusLost,
}

/// A mouse button.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Button {
    /// Left.
    Left,
    /// Middle.
    Middle,
    /// Right.
    Right,
}

/// What the mouse did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseKind {
    /// Button pressed.
    Press(Button),
    /// Button released.
    Release(Button),
    /// Moved with the button held.
    Drag(Button),
    /// Wheel up.
    ScrollUp,
    /// Wheel down.
    ScrollDown,
}

/// A mouse event at a zero-based cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Mouse {
    kind: MouseKind,
    column: u16,
    row: u16,
    mods: Mods,
}

impl Mouse {
    /// A mouse event.
    #[must_use]
    pub const fn new(kind: MouseKind, column: u16, row: u16, mods: Mods) -> Self {
        Self {
            kind,
            column,
            row,
            mods,
        }
    }

    /// What happened.
    #[must_use]
    pub const fn kind(self) -> MouseKind {
        self.kind
    }

    /// Zero-based column.
    #[must_use]
    pub const fn column(self) -> u16 {
        self.column
    }

    /// Zero-based row.
    #[must_use]
    pub const fn row(self) -> u16 {
        self.row
    }

    /// Held modifiers.
    #[must_use]
    pub const fn mods(self) -> Mods {
        self.mods
    }
}

/// Decodes bytes into inputs across reads.
#[derive(Debug, Default)]
pub struct Decoder {
    pending: Vec<u8>,
}

impl Decoder {
    /// A decoder with nothing pending.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether bytes are waiting for the rest of a sequence.
    #[must_use]
    pub fn has_pending(&self) -> bool {
        !self.pending.is_empty()
    }

    /// Decodes `bytes`, keeping an incomplete tail for the next call.
    pub fn feed(&mut self, bytes: &[u8]) -> Vec<Input> {
        let _ = bytes;
        Vec::new()
    }

    /// Ends whatever is pending after `ESC_TIMEOUT` of silence: a lone `ESC` is the Esc key, any
    /// other incomplete sequence is dropped.
    pub fn flush(&mut self) -> Vec<Input> {
        self.pending.clear();
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use pawsman_core::keys::{Chord, KeyCode, Mods};
    use rstest::rstest;

    use super::*;

    fn keys(bytes: &[u8]) -> Vec<Input> {
        let mut d = Decoder::new();
        let mut out = d.feed(bytes);
        out.extend(d.flush());
        out
    }

    fn key(code: KeyCode, mods: Mods) -> Input {
        Input::Key(Chord::new(code, mods))
    }

    #[rstest]
    #[case::letter(b"a", key(KeyCode::Char('a'), Mods::NONE))]
    #[case::shifted_text(b"A", key(KeyCode::Char('A'), Mods::NONE))]
    #[case::cyrillic(b"\xd0\xb6", key(KeyCode::Char('\u{436}'), Mods::NONE))]
    #[case::enter(b"\r", key(KeyCode::Enter, Mods::NONE))]
    #[case::tab(b"\t", key(KeyCode::Tab, Mods::NONE))]
    #[case::backspace(b"\x7f", key(KeyCode::Backspace, Mods::NONE))]
    #[case::ctrl_q_legacy(b"\x11", key(KeyCode::Char('q'), Mods::CTRL))]
    #[case::ctrl_c_legacy(b"\x03", key(KeyCode::Char('c'), Mods::CTRL))]
    #[case::lone_esc(b"\x1b", key(KeyCode::Esc, Mods::NONE))]
    #[case::alt_x(b"\x1bx", key(KeyCode::Char('x'), Mods::ALT))]
    #[case::alt_enter(b"\x1b\r", key(KeyCode::Enter, Mods::ALT))]
    #[case::up(b"\x1b[A", key(KeyCode::Up, Mods::NONE))]
    #[case::ss3_up(b"\x1bOA", key(KeyCode::Up, Mods::NONE))]
    #[case::ctrl_right(b"\x1b[1;5C", key(KeyCode::Right, Mods::CTRL))]
    #[case::f1_ss3(b"\x1bOP", key(KeyCode::F(1), Mods::NONE))]
    #[case::f5(b"\x1b[15~", key(KeyCode::F(5), Mods::NONE))]
    #[case::shift_f5(b"\x1b[15;2~", key(KeyCode::F(5), Mods::SHIFT))]
    #[case::delete(b"\x1b[3~", key(KeyCode::Delete, Mods::NONE))]
    #[case::backtab(b"\x1b[Z", key(KeyCode::BackTab, Mods::NONE))]
    #[case::kitty_ctrl_q(b"\x1b[113;5u", key(KeyCode::Char('q'), Mods::CTRL))]
    #[case::kitty_ctrl_enter(b"\x1b[13;5u", key(KeyCode::Enter, Mods::CTRL))]
    #[case::kitty_esc(b"\x1b[27u", key(KeyCode::Esc, Mods::NONE))]
    #[case::kitty_cyrillic_ctrl(b"\x1b[1081;5u", key(KeyCode::Char('\u{439}'), Mods::CTRL))]
    #[case::kitty_caps_lock_ignored(b"\x1b[113;69u", key(KeyCode::Char('q'), Mods::CTRL))]
    #[case::kitty_f1(b"\x1b[P", key(KeyCode::F(1), Mods::NONE))]
    fn one_key(#[case] bytes: &[u8], #[case] expected: Input) {
        assert_eq!(keys(bytes), vec![expected], "bytes {bytes:?}");
    }

    #[test]
    fn kitty_release_events_are_dropped() {
        assert_eq!(keys(b"\x1b[113;5:3u"), vec![], "release");
    }

    #[rstest]
    #[case::graphics(b"\x1b_Gi=31;OK\x1b\\")]
    #[case::keyboard_flags(b"\x1b[?1u")]
    #[case::da1(b"\x1b[?62;22c")]
    #[case::cell_size(b"\x1b[6;20;10t")]
    #[case::decrqss(b"\x1bP1$r48:2:1:2:3m\x1b\\")]
    #[case::osc(b"\x1b]52;c;aGk=\x07")]
    fn late_probe_replies_type_nothing(#[case] bytes: &[u8]) {
        let mut with_key = bytes.to_vec();
        with_key.extend_from_slice(b"z");

        assert_eq!(
            keys(&with_key),
            vec![key(KeyCode::Char('z'), Mods::NONE)],
            "only the key after {bytes:?}"
        );
    }

    #[test]
    fn paste_arrives_whole_with_control_bytes_inside() {
        let out = keys(b"\x1b[200~line one\r\n\x1b[Ax\x1b[201~q");

        assert_eq!(
            out,
            vec![
                Input::Paste("line one\r\n\x1b[Ax".to_owned()),
                key(KeyCode::Char('q'), Mods::NONE)
            ],
            "paste then key"
        );
    }

    #[test]
    fn paste_split_across_reads_including_its_end_marker() {
        let mut d = Decoder::new();

        let mut out = d.feed(b"\x1b[200~ab");
        out.extend(d.feed(b"c\x1b[20"));
        out.extend(d.feed(b"1~"));

        assert_eq!(out, vec![Input::Paste("abc".to_owned())], "one paste");
    }

    #[test]
    fn sequences_split_across_reads_decode_once() {
        let mut d = Decoder::new();

        let first = d.feed(b"\x1b[11");
        let second = d.feed(b"3;5u");

        assert_eq!(first, vec![], "nothing yet");
        assert_eq!(
            second,
            vec![key(KeyCode::Char('q'), Mods::CTRL)],
            "whole chord"
        );
    }

    #[test]
    fn split_utf8_waits_for_the_rest() {
        let mut d = Decoder::new();

        let first = d.feed(b"\xd0");
        let second = d.feed(b"\xb6");

        assert_eq!(first, vec![], "half a character");
        assert_eq!(
            second,
            vec![key(KeyCode::Char('\u{436}'), Mods::NONE)],
            "whole character"
        );
    }

    #[test]
    fn focus_and_sgr_mouse() {
        let out = keys(b"\x1b[I\x1b[<0;10;5M\x1b[<64;1;1M\x1b[<32;11;5M\x1b[<0;11;5m\x1b[O");

        assert_eq!(
            out,
            vec![
                Input::FocusGained,
                Input::Mouse(Mouse::new(MouseKind::Press(Button::Left), 9, 4, Mods::NONE)),
                Input::Mouse(Mouse::new(MouseKind::ScrollUp, 0, 0, Mods::NONE)),
                Input::Mouse(Mouse::new(MouseKind::Drag(Button::Left), 10, 4, Mods::NONE)),
                Input::Mouse(Mouse::new(
                    MouseKind::Release(Button::Left),
                    10,
                    4,
                    Mods::NONE
                )),
                Input::FocusLost,
            ],
            "focus, press, wheel, drag, release, blur"
        );
    }

    #[test]
    fn unterminated_string_is_dropped_at_the_cap() {
        let mut d = Decoder::new();
        let mut junk = b"\x1bP".to_vec();
        junk.resize(crate::input::MAX_SEQUENCE + 10, b'x');

        let during = d.feed(&junk);
        let after = d.feed(b"\x1b[A");

        assert_eq!(during, vec![], "the string types nothing");
        assert_eq!(
            after,
            vec![key(KeyCode::Up, Mods::NONE)],
            "keys work again after the cap"
        );
    }

    #[test]
    fn paste_is_capped() {
        let mut d = Decoder::new();
        let mut paste = b"\x1b[200~".to_vec();
        paste.resize(crate::input::MAX_PASTE + 100, b'a');
        paste.extend_from_slice(b"\x1b[201~");

        let out = d.feed(&paste);

        let [Input::Paste(text)] = out.as_slice() else {
            panic!("one paste, got {} inputs", out.len())
        };
        assert_eq!(text.len(), crate::input::MAX_PASTE, "capped");
    }

    #[test]
    fn pending_only_while_a_sequence_is_open() {
        let mut d = Decoder::new();

        d.feed(b"\x1b");
        let open = d.has_pending();
        d.flush();

        assert!(open, "a lone ESC is pending");
        assert!(!d.has_pending(), "flushed");
    }
}
