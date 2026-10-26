//! The key table. The hint bar, the F1 overlay and dispatch all read these rows.

mod chord;
mod fold;

pub use chord::{Chord, KeyCode, Mods};
pub use fold::fold;

/// What a key does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Action {
    /// Quit pawsman.
    Quit,
    /// Open the help overlay.
    Help,
    /// Close the overlay.
    Close,
    /// Copy the selection.
    Copy,
    /// Send the selected request.
    Send,
    /// Cancel the selected request.
    Cancel,
    /// Focus the next pane.
    NextPane,
    /// Focus the previous pane.
    PrevPane,
    /// Move up.
    Up,
    /// Move down.
    Down,
    /// Scroll a page up.
    PageUp,
    /// Scroll a page down.
    PageDown,
}

/// Where a binding applies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Context {
    /// The startup splash.
    Splash,
    /// The view without a collection.
    Home,
    /// The collection view: sidebar, request and response.
    Main,
    /// The help overlay.
    Help,
}

/// Where a binding is shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Shown {
    Bar,
    HelpOnly,
}

/// One row of the key table.
#[derive(Debug)]
pub struct Binding {
    action: Action,
    chords: &'static [Chord],
    contexts: &'static [Context],
    hint: &'static str,
    shown: Shown,
}

impl Binding {
    /// The action.
    #[must_use]
    pub fn action(&self) -> Action {
        self.action
    }

    /// The keys, the first one shown in the hint bar.
    #[must_use]
    pub fn chords(&self) -> &'static [Chord] {
        self.chords
    }

    /// The short description.
    #[must_use]
    pub fn hint(&self) -> &'static str {
        self.hint
    }

    /// Shown in the hint bar, not only in the F1 overlay.
    #[must_use]
    pub fn in_bar(&self) -> bool {
        self.shown == Shown::Bar
    }
}

const ALL: &[Context] = &[Context::Splash, Context::Home, Context::Main, Context::Help];

static TABLE: &[Binding] = &[
    Binding {
        action: Action::Send,
        chords: &[
            Chord::plain(KeyCode::F(5)),
            Chord::new(KeyCode::Enter, Mods::CTRL),
            Chord::new(KeyCode::Enter, Mods::ALT),
        ],
        contexts: &[Context::Main],
        hint: "send",
        shown: Shown::Bar,
    },
    Binding {
        action: Action::NextPane,
        chords: &[Chord::plain(KeyCode::Tab)],
        contexts: &[Context::Main],
        hint: "pane",
        shown: Shown::Bar,
    },
    Binding {
        action: Action::Cancel,
        chords: &[Chord::plain(KeyCode::Esc)],
        contexts: &[Context::Main],
        hint: "cancel",
        shown: Shown::Bar,
    },
    Binding {
        action: Action::PrevPane,
        chords: &[Chord::plain(KeyCode::BackTab)],
        contexts: &[Context::Main],
        hint: "previous pane",
        shown: Shown::HelpOnly,
    },
    Binding {
        action: Action::Up,
        chords: &[Chord::plain(KeyCode::Up), Chord::plain(KeyCode::Char('k'))],
        contexts: &[Context::Main],
        hint: "up",
        shown: Shown::HelpOnly,
    },
    Binding {
        action: Action::Down,
        chords: &[
            Chord::plain(KeyCode::Down),
            Chord::plain(KeyCode::Char('j')),
        ],
        contexts: &[Context::Main],
        hint: "down",
        shown: Shown::HelpOnly,
    },
    Binding {
        action: Action::PageUp,
        chords: &[Chord::plain(KeyCode::PageUp)],
        contexts: &[Context::Main],
        hint: "page up",
        shown: Shown::HelpOnly,
    },
    Binding {
        action: Action::PageDown,
        chords: &[Chord::plain(KeyCode::PageDown)],
        contexts: &[Context::Main],
        hint: "page down",
        shown: Shown::HelpOnly,
    },
    Binding {
        action: Action::Quit,
        chords: &[Chord::ctrl('q')],
        contexts: ALL,
        hint: "quit",
        shown: Shown::Bar,
    },
    Binding {
        action: Action::Help,
        chords: &[Chord::plain(KeyCode::F(1))],
        contexts: &[Context::Home, Context::Main],
        hint: "help",
        shown: Shown::Bar,
    },
    Binding {
        action: Action::Close,
        chords: &[Chord::plain(KeyCode::Esc), Chord::plain(KeyCode::F(1))],
        contexts: &[Context::Help],
        hint: "close",
        shown: Shown::Bar,
    },
    Binding {
        action: Action::Copy,
        chords: &[Chord::ctrl('c')],
        contexts: &[Context::Home, Context::Main],
        hint: "copy the selection",
        shown: Shown::HelpOnly,
    },
];

/// Every binding.
#[must_use]
pub fn table() -> &'static [Binding] {
    TABLE
}

/// The action bound to `chord` in `context`, with layout folding.
#[must_use]
pub fn lookup(context: Context, chord: Chord) -> Option<Action> {
    let folded = match chord.code() {
        KeyCode::Char(c) => Chord::new(KeyCode::Char(fold(c)), chord.mods()),
        _ => chord,
    };
    TABLE
        .iter()
        .find(|b| {
            b.contexts.contains(&context) && b.chords.iter().any(|c| *c == chord || *c == folded)
        })
        .map(Binding::action)
}

/// `(key label, hint)` pairs for the hint bar in `context`.
#[must_use]
pub fn hints(context: Context) -> Vec<(String, &'static str)> {
    TABLE
        .iter()
        .filter(|b| b.in_bar() && b.contexts.contains(&context))
        .filter_map(|b| Some((b.chords.first()?.to_string(), b.hint)))
        .collect()
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    #[rstest]
    #[case::ctrl_q(Context::Home, Chord::ctrl('q'), Some(Action::Quit))]
    #[case::quit_in_splash(Context::Splash, Chord::ctrl('q'), Some(Action::Quit))]
    #[case::quit_in_help(Context::Help, Chord::ctrl('q'), Some(Action::Quit))]
    #[case::f1_help(Context::Home, Chord::plain(KeyCode::F(1)), Some(Action::Help))]
    #[case::esc_closes_help(Context::Help, Chord::plain(KeyCode::Esc), Some(Action::Close))]
    #[case::f1_closes_help(Context::Help, Chord::plain(KeyCode::F(1)), Some(Action::Close))]
    #[case::ctrl_c_copies(Context::Home, Chord::ctrl('c'), Some(Action::Copy))]
    #[case::plain_q_is_nothing(Context::Home, Chord::plain(KeyCode::Char('q')), None)]
    #[case::esc_on_home_is_nothing(Context::Home, Chord::plain(KeyCode::Esc), None)]
    fn lookup_finds_the_bound_action(
        #[case] context: Context,
        #[case] chord: Chord,
        #[case] expected: Option<Action>,
    ) {
        assert_eq!(lookup(context, chord), expected, "{chord:?} in {context:?}");
    }

    #[test]
    fn cyrillic_ctrl_q_quits() {
        let chord = Chord::new(KeyCode::Char('й'), Mods::CTRL);

        assert_eq!(
            lookup(Context::Home, chord),
            Some(Action::Quit),
            "Ctrl-й is Ctrl-Q"
        );
    }

    #[test]
    fn shifted_cyrillic_folds_to_lowercase_us() {
        assert_eq!(fold('Й'), 'q', "capital folds too");
        assert_eq!(fold('ж'), ';', "punctuation position");
        assert_eq!(fold('x'), 'x', "latin unchanged");
    }

    #[rstest]
    #[case(Chord::ctrl('q'), "Ctrl-Q")]
    #[case(Chord::plain(KeyCode::F(1)), "F1")]
    #[case(Chord::plain(KeyCode::Esc), "Esc")]
    #[case(Chord::new(KeyCode::Enter, Mods::ALT), "Alt-Enter")]
    #[case(Chord::new(KeyCode::Char('k'), Mods::CTRL | Mods::SHIFT), "Ctrl-Shift-K")]
    #[case(Chord::plain(KeyCode::Char('k')), "k")]
    fn chords_display_like_the_docs(#[case] chord: Chord, #[case] expected: &str) {
        assert_eq!(chord.to_string(), expected, "label");
    }

    #[test]
    fn home_hints_name_quit_and_help() {
        let hints = hints(Context::Home);

        assert_eq!(
            hints,
            vec![("Ctrl-Q".to_owned(), "quit"), ("F1".to_owned(), "help")],
            "bar hints on home"
        );
    }

    #[test]
    fn every_binding_has_keys_and_a_hint() {
        for binding in table() {
            assert!(
                !binding.chords().is_empty(),
                "{:?} has keys",
                binding.action()
            );
            assert!(
                !binding.hint().is_empty(),
                "{:?} has a hint",
                binding.action()
            );
        }
        assert!(
            table()
                .iter()
                .any(|b| b.action() == Action::Quit && b.hint() == "quit"),
            "quit is in the table"
        );
    }

    #[rstest]
    #[case::f5(Chord::plain(KeyCode::F(5)), Action::Send)]
    #[case::ctrl_enter(Chord::new(KeyCode::Enter, Mods::CTRL), Action::Send)]
    #[case::alt_enter(Chord::new(KeyCode::Enter, Mods::ALT), Action::Send)]
    #[case::esc(Chord::plain(KeyCode::Esc), Action::Cancel)]
    #[case::tab(Chord::plain(KeyCode::Tab), Action::NextPane)]
    #[case::backtab(Chord::plain(KeyCode::BackTab), Action::PrevPane)]
    #[case::up(Chord::plain(KeyCode::Up), Action::Up)]
    #[case::k(Chord::plain(KeyCode::Char('k')), Action::Up)]
    #[case::down(Chord::plain(KeyCode::Down), Action::Down)]
    #[case::j(Chord::plain(KeyCode::Char('j')), Action::Down)]
    #[case::cyrillic_j(Chord::plain(KeyCode::Char('о')), Action::Down)]
    #[case::page_down(Chord::plain(KeyCode::PageDown), Action::PageDown)]
    #[case::page_up(Chord::plain(KeyCode::PageUp), Action::PageUp)]
    #[case::quit(Chord::ctrl('q'), Action::Quit)]
    #[case::help(Chord::plain(KeyCode::F(1)), Action::Help)]
    fn main_view_bindings(#[case] chord: Chord, #[case] expected: Action) {
        assert_eq!(lookup(Context::Main, chord), Some(expected), "{chord}");
    }

    #[test]
    fn main_hints_lead_with_send() {
        let hints = hints(Context::Main);

        let keys: Vec<&str> = hints.iter().map(|(k, _)| k.as_str()).collect();
        assert_eq!(keys, ["F5", "Tab", "Esc", "Ctrl-Q", "F1"], "{hints:?}");
    }

    #[test]
    fn main_keys_do_nothing_on_home() {
        assert_eq!(
            lookup(Context::Home, Chord::plain(KeyCode::F(5))),
            None,
            "no send without a collection"
        );
    }

    #[test]
    fn every_context_can_quit() {
        for context in [Context::Splash, Context::Home, Context::Main, Context::Help] {
            assert_eq!(
                lookup(context, Chord::ctrl('q')),
                Some(Action::Quit),
                "{context:?}"
            );
        }
    }
}
