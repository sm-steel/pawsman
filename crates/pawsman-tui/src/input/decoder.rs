//! A pure, stateful decoder from terminal bytes to inputs: legacy keys, kitty `CSI … u` under the
//! disambiguate flag, SGR mouse, bracketed paste and focus. Terminal replies (APC, DCS, OSC, and the
//! probe's CSI replies) are dropped, so a late probe reply never types anything.

use pawsman_core::keys::{Chord, KeyCode, Mods};
use tracing::debug;

use super::{ESC_TIMEOUT, MAX_PASTE, MAX_SEQUENCE, PASTE_TIMEOUT};

const ESC: u8 = 0x1b;
const PASTE_END: &[u8] = b"\x1b[201~";

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
    paste: Option<Vec<u8>>,
}

/// What one parse step found at the front of `pending`.
enum Step {
    /// Consumed `usize` bytes, maybe producing an input.
    Done(usize, Option<Input>),
    /// Needs more bytes.
    Incomplete,
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
        !self.pending.is_empty() || self.paste.is_some()
    }

    /// Decodes `bytes`, keeping an incomplete tail for the next call.
    pub fn feed(&mut self, bytes: &[u8]) -> Vec<Input> {
        self.pending.extend_from_slice(bytes);
        let mut out = Vec::new();
        while !self.pending.is_empty() {
            if self.paste.is_some() {
                match self.take_paste() {
                    Some(text) => {
                        out.push(Input::Paste(text));
                        continue;
                    }
                    None => break,
                }
            }
            match self.step() {
                Step::Done(n, input) => {
                    self.pending.drain(..n.min(self.pending.len()));
                    out.extend(input);
                }
                Step::Incomplete => {
                    if self.pending.len() > MAX_SEQUENCE {
                        debug!(bytes = self.pending.len(), "dropped an over-long sequence");
                        self.pending.clear();
                    }
                    break;
                }
            }
        }
        out
    }

    /// How long a silence ends what is pending: `PASTE_TIMEOUT` inside a paste, else `ESC_TIMEOUT`.
    #[must_use]
    pub fn silence(&self) -> std::time::Duration {
        if self.paste.is_some() {
            PASTE_TIMEOUT
        } else {
            ESC_TIMEOUT
        }
    }

    /// Ends whatever is pending after `ESC_TIMEOUT` of silence: a lone `ESC` is the Esc key, any
    /// other incomplete sequence is dropped. A paste still open after `PASTE_TIMEOUT` ends with what
    /// arrived, so a lost end marker never swallows later keys.
    pub fn flush(&mut self) -> Vec<Input> {
        if let Some(mut bytes) = self.paste.take() {
            let room = MAX_PASTE.saturating_sub(bytes.len());
            bytes.extend(self.pending.drain(..).take(room));
            debug!(bytes = bytes.len(), "closed a paste without its end marker");
            return vec![Input::Paste(String::from_utf8_lossy(&bytes).into_owned())];
        }
        let lone_esc = self.pending == [ESC];
        self.pending.clear();
        if lone_esc {
            vec![Input::Key(Chord::plain(KeyCode::Esc))]
        } else {
            Vec::new()
        }
    }

    /// Moves paste bytes out of `pending`, up to `MAX_PASTE`; the text once the end marker arrived.
    fn take_paste(&mut self) -> Option<String> {
        let end = find(&self.pending, PASTE_END);
        // Without the end marker, keep a tail that may be its start.
        let take = end.unwrap_or_else(|| self.pending.len().saturating_sub(PASTE_END.len() - 1));
        let paste = self.paste.get_or_insert_with(Vec::new);
        let room = MAX_PASTE.saturating_sub(paste.len());
        paste.extend(self.pending.iter().take(take.min(room)));
        self.pending.drain(..take);
        end?;
        self.pending.drain(..PASTE_END.len());
        self.paste
            .take()
            .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
    }

    fn step(&mut self) -> Step {
        let Some(&first) = self.pending.first() else {
            return Step::Incomplete;
        };
        if first != ESC {
            return plain(&self.pending, Mods::NONE, 0);
        }
        match self.pending.get(1) {
            None => Step::Incomplete,
            Some(b'[') => self.csi(),
            Some(b'O') => ss3(&self.pending),
            Some(b'_' | b'P' | b']' | b'X' | b'^') => string(&self.pending),
            Some(&ESC) => Step::Done(1, Some(Input::Key(Chord::new(KeyCode::Esc, Mods::ALT)))),
            Some(_) => plain(&self.pending, Mods::ALT, 1),
        }
    }

    /// `ESC [ params final`; starts a paste on `200~`.
    fn csi(&mut self) -> Step {
        let body = self.pending.get(2..).unwrap_or_default();
        let Some(fin) = body.iter().position(|b| (0x40..=0x7e).contains(b)) else {
            return Step::Incomplete;
        };
        let params = body.get(..fin).unwrap_or_default();
        let final_byte = body.get(fin).copied().unwrap_or(0);
        let len = 2 + fin + 1;
        if final_byte == b'~' && params == b"200" {
            self.paste = Some(Vec::new());
            return Step::Done(len, None);
        }
        Step::Done(len, csi_input(params, final_byte))
    }
}

/// A plain byte or UTF-8 character at `pending[at..]`, with `mods` (Alt after a leading ESC).
fn plain(pending: &[u8], mods: Mods, at: usize) -> Step {
    let Some(&b) = pending.get(at) else {
        return Step::Incomplete;
    };
    let chord = |code, extra: Mods| Some(Input::Key(Chord::new(code, mods | extra)));
    let input = match b {
        b'\r' | b'\n' => chord(KeyCode::Enter, Mods::NONE),
        b'\t' => chord(KeyCode::Tab, Mods::NONE),
        0x7f | 0x08 => chord(KeyCode::Backspace, Mods::NONE),
        0x00 => chord(KeyCode::Char(' '), Mods::CTRL),
        0x01..=0x1a => chord(KeyCode::Char(char::from(b'a' + b - 1)), Mods::CTRL),
        0x1c..=0x1f => chord(KeyCode::Char(char::from(b'\\' + b - 0x1c)), Mods::CTRL),
        _ if b < 0x80 => chord(KeyCode::Char(char::from(b)), Mods::NONE),
        _ => {
            let width = utf8_width(b);
            let Some(bytes) = pending.get(at..at + width) else {
                return Step::Incomplete;
            };
            let Some(c) = std::str::from_utf8(bytes)
                .ok()
                .and_then(|s| s.chars().next())
            else {
                return Step::Done(at + 1, None);
            };
            return Step::Done(at + width, chord(KeyCode::Char(c), Mods::NONE));
        }
    };
    Step::Done(at + 1, input)
}

fn utf8_width(lead: u8) -> usize {
    match lead {
        0xc0..=0xdf => 2,
        0xe0..=0xef => 3,
        0xf0..=0xf7 => 4,
        _ => 1,
    }
}

/// `ESC O <final>`: application-mode arrows, Home, End, F1–F4.
fn ss3(pending: &[u8]) -> Step {
    let Some(&fin) = pending.get(2) else {
        return Step::Incomplete;
    };
    let code = match fin {
        b'A' => KeyCode::Up,
        b'B' => KeyCode::Down,
        b'C' => KeyCode::Right,
        b'D' => KeyCode::Left,
        b'H' => KeyCode::Home,
        b'F' => KeyCode::End,
        b'P'..=b'S' => KeyCode::F(fin - b'P' + 1),
        _ => return Step::Done(3, None),
    };
    Step::Done(3, Some(Input::Key(Chord::plain(code))))
}

/// APC, DCS, OSC, SOS, PM: dropped up to `ESC \` (or BEL for OSC).
fn string(pending: &[u8]) -> Step {
    let st = find(pending, b"\x1b\\").map(|i| i + 2);
    let bel = if pending.get(1) == Some(&b']') {
        pending.iter().position(|&b| b == 0x07).map(|i| i + 1)
    } else {
        None
    };
    match st.into_iter().chain(bel).min() {
        Some(n) => {
            debug!(
                kind = %char::from(pending.get(1).copied().unwrap_or(b'?')),
                "dropped a terminal reply"
            );
            Step::Done(n, None)
        }
        None => Step::Incomplete,
    }
}

fn csi_input(params: &[u8], final_byte: u8) -> Option<Input> {
    if params.first() == Some(&b'?') {
        debug!(final_byte = %char::from(final_byte), "dropped a terminal reply");
        return None;
    }
    if params.first() == Some(&b'<') {
        return sgr_mouse(params.get(1..).unwrap_or_default(), final_byte);
    }
    let text = std::str::from_utf8(params).ok()?;
    let mut fields = text.split(';');
    let first = fields.next().unwrap_or("");
    let second = fields.next().unwrap_or("");
    let mods = mods_field(second);
    let chord = |code| Some(Input::Key(Chord::new(code, mods)));
    match final_byte {
        b'u' => kitty(first, second),
        b'A' => chord(KeyCode::Up),
        b'B' => chord(KeyCode::Down),
        b'C' => chord(KeyCode::Right),
        b'D' => chord(KeyCode::Left),
        b'H' => chord(KeyCode::Home),
        b'F' => chord(KeyCode::End),
        b'P'..=b'S' => chord(KeyCode::F(final_byte - b'P' + 1)),
        b'Z' => Some(Input::Key(Chord::plain(KeyCode::BackTab))),
        b'I' => Some(Input::FocusGained),
        b'O' => Some(Input::FocusLost),
        b'~' => tilde(first.split(':').next()?.parse().ok()?).and_then(chord),
        b't' | b'c' | b'n' | b'y' => {
            debug!(final_byte = %char::from(final_byte), "dropped a terminal reply");
            None
        }
        _ => None,
    }
}

/// The modifier field `1 + bits[:event]`; missing means none.
fn mods_field(field: &str) -> Mods {
    let value = field
        .split(':')
        .next()
        .and_then(|n| n.parse::<u16>().ok())
        .unwrap_or(1);
    Mods::from_bits(u8::try_from(value.saturating_sub(1) & 0xff).unwrap_or(0))
}

fn tilde(code: u32) -> Option<KeyCode> {
    let f = |offset: u32| u8::try_from(code - offset).ok().map(KeyCode::F);
    match code {
        1 | 7 => Some(KeyCode::Home),
        2 => Some(KeyCode::Insert),
        3 => Some(KeyCode::Delete),
        4 | 8 => Some(KeyCode::End),
        5 => Some(KeyCode::PageUp),
        6 => Some(KeyCode::PageDown),
        11..=15 => f(10),
        17..=21 => f(11),
        23..=26 => f(12),
        28 | 29 => f(13),
        31..=34 => f(14),
        _ => None,
    }
}

/// `CSI code[:alternates] ; mods[:event] u`; release events (event 3) are dropped.
fn kitty(first: &str, second: &str) -> Option<Input> {
    let code: u32 = first.split(':').next()?.parse().ok()?;
    if second.split(':').nth(1) == Some("3") {
        return None;
    }
    let key = match code {
        13 | 57_414 => KeyCode::Enter,
        9 => KeyCode::Tab,
        27 => KeyCode::Esc,
        127 => KeyCode::Backspace,
        57_344..=57_454 => return None,
        _ => KeyCode::Char(char::from_u32(code)?),
    };
    Some(Input::Key(Chord::new(key, mods_field(second))))
}

/// `CSI < b ; x ; y (M|m)`.
fn sgr_mouse(params: &[u8], final_byte: u8) -> Option<Input> {
    let text = std::str::from_utf8(params).ok()?;
    let mut nums = text.split(';').map(|n| n.parse::<u16>().ok());
    let (b, x, y) = (nums.next()??, nums.next()??, nums.next()??);
    let mut mods = Mods::NONE;
    for (bit, m) in [(4, Mods::SHIFT), (8, Mods::ALT), (16, Mods::CTRL)] {
        if b & bit != 0 {
            mods = mods | m;
        }
    }
    let button = match b & 0b11 {
        0 => Button::Left,
        1 => Button::Middle,
        _ => Button::Right,
    };
    let kind = if b & 64 != 0 {
        if b & 1 == 0 {
            MouseKind::ScrollUp
        } else {
            MouseKind::ScrollDown
        }
    } else if b & 32 != 0 {
        MouseKind::Drag(button)
    } else if final_byte == b'm' {
        MouseKind::Release(button)
    } else {
        MouseKind::Press(button)
    };
    Some(Input::Mouse(Mouse::new(
        kind,
        x.saturating_sub(1),
        y.saturating_sub(1),
        mods,
    )))
}

fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
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
    fn an_unterminated_paste_ends_at_flush_and_keys_work_again() {
        let mut d = Decoder::new();

        let during = d.feed(b"\x1b[200~ab");
        let flushed = d.flush();
        let after = d.feed(b"\x11");

        assert_eq!(during, vec![], "the paste is open");
        assert_eq!(
            flushed,
            vec![Input::Paste("ab".to_owned())],
            "closed with what arrived"
        );
        assert_eq!(
            after,
            vec![key(KeyCode::Char('q'), Mods::CTRL)],
            "Ctrl-Q is a key again"
        );
    }

    #[test]
    fn the_silence_is_longer_inside_a_paste() {
        let mut d = Decoder::new();

        d.feed(b"\x1b");
        let escape = d.silence();
        d.flush();
        d.feed(b"\x1b[200~a");
        let paste = d.silence();

        assert_eq!(
            escape,
            crate::input::ESC_TIMEOUT,
            "a lone ESC waits briefly"
        );
        assert_eq!(
            paste,
            crate::input::PASTE_TIMEOUT,
            "a paste waits for its end marker"
        );
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
