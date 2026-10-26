//! A key with its modifiers, independent of any terminal library.

use std::fmt;
use std::ops::BitOr;

/// Modifier bits, as in the kitty keyboard protocol (value minus one).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Mods(u8);

impl Mods {
    /// No modifier.
    pub const NONE: Self = Self(0);
    /// Shift.
    pub const SHIFT: Self = Self(1);
    /// Alt (Option).
    pub const ALT: Self = Self(2);
    /// Control.
    pub const CTRL: Self = Self(4);
    /// Super (Cmd, Windows).
    pub const SUPER: Self = Self(8);
    /// Hyper.
    pub const HYPER: Self = Self(16);
    /// Meta.
    pub const META: Self = Self(32);

    /// Keeps the six modifier bits; Caps Lock and Num Lock are dropped.
    #[must_use]
    pub const fn from_bits(bits: u8) -> Self {
        Self(bits & 0b11_1111)
    }

    /// All of `other` is set.
    #[must_use]
    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    /// No modifier is set.
    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }
}

impl BitOr for Mods {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self {
        Self(self.0 | rhs.0)
    }
}

/// A key, without modifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum KeyCode {
    /// A character key; letters are lowercase when Ctrl or Alt is held.
    Char(char),
    /// Enter.
    Enter,
    /// Escape.
    Esc,
    /// Tab.
    Tab,
    /// Shift-Tab.
    BackTab,
    /// Backspace.
    Backspace,
    /// Up arrow.
    Up,
    /// Down arrow.
    Down,
    /// Left arrow.
    Left,
    /// Right arrow.
    Right,
    /// Home.
    Home,
    /// End.
    End,
    /// Page Up.
    PageUp,
    /// Page Down.
    PageDown,
    /// Insert.
    Insert,
    /// Delete.
    Delete,
    /// A function key, F1 to F24.
    F(u8),
}

/// A key and its modifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Chord {
    code: KeyCode,
    mods: Mods,
}

impl Chord {
    /// A key with modifiers.
    #[must_use]
    pub const fn new(code: KeyCode, mods: Mods) -> Self {
        Self { code, mods }
    }

    /// A key without modifiers.
    #[must_use]
    pub const fn plain(code: KeyCode) -> Self {
        Self {
            code,
            mods: Mods::NONE,
        }
    }

    /// Ctrl plus a lowercase character.
    #[must_use]
    pub const fn ctrl(c: char) -> Self {
        Self {
            code: KeyCode::Char(c),
            mods: Mods::CTRL,
        }
    }

    /// The key.
    #[must_use]
    pub const fn code(self) -> KeyCode {
        self.code
    }

    /// The modifiers.
    #[must_use]
    pub const fn mods(self) -> Mods {
        self.mods
    }
}

impl fmt::Display for Chord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (bit, name) in [
            (Mods::CTRL, "Ctrl-"),
            (Mods::ALT, "Alt-"),
            (Mods::SHIFT, "Shift-"),
            (Mods::SUPER, "Super-"),
        ] {
            if self.mods.contains(bit) {
                f.write_str(name)?;
            }
        }
        let name = match self.code {
            KeyCode::Char(' ') => "Space",
            KeyCode::Char(c) => return write!(f, "{}", c.to_uppercase()),
            KeyCode::F(n) => return write!(f, "F{n}"),
            KeyCode::Enter => "Enter",
            KeyCode::Esc => "Esc",
            KeyCode::Tab => "Tab",
            KeyCode::BackTab => "Shift-Tab",
            KeyCode::Backspace => "Backspace",
            KeyCode::Up => "Up",
            KeyCode::Down => "Down",
            KeyCode::Left => "Left",
            KeyCode::Right => "Right",
            KeyCode::Home => "Home",
            KeyCode::End => "End",
            KeyCode::PageUp => "PgUp",
            KeyCode::PageDown => "PgDn",
            KeyCode::Insert => "Ins",
            KeyCode::Delete => "Del",
        };
        f.write_str(name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_bits_drops_caps_and_num_lock() {
        assert_eq!(
            Mods::from_bits(64 | 128 | 4),
            Mods::CTRL,
            "lock bits masked"
        );
        assert_eq!(
            Mods::from_bits(0b11_1111),
            Mods::from_bits(63),
            "all six kept"
        );
        assert!(
            Mods::from_bits(64).is_empty(),
            "caps lock alone is no modifier"
        );
    }

    #[test]
    fn union_and_contains() {
        let both = Mods::CTRL | Mods::SHIFT;

        assert!(
            both.contains(Mods::CTRL) && both.contains(Mods::SHIFT),
            "both set"
        );
        assert!(!both.contains(Mods::ALT), "alt not set");
        assert_eq!(Mods::CTRL | Mods::CTRL, Mods::CTRL, "idempotent");
        assert!(Mods::NONE.is_empty() && !Mods::ALT.is_empty(), "is_empty");
    }
}
