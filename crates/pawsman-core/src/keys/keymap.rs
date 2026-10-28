//! The bindings in use: the defaults, with `keys.toml` overrides applied.

use std::path::Path;

use serde::Deserialize;

use super::{Action, Chord, Context, KeyCode, Mods, fold};
use crate::CoreError;

/// Where a binding is shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Shown {
    Bar,
    HelpOnly,
}

/// One row of the key table.
#[derive(Debug, Clone)]
pub struct Binding {
    action: Action,
    chords: Vec<Chord>,
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
    pub fn chords(&self) -> &[Chord] {
        &self.chords
    }

    /// The contexts it applies in.
    #[must_use]
    pub fn contexts(&self) -> &'static [Context] {
        self.contexts
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

/// A default row.
struct Row {
    action: Action,
    chords: &'static [Chord],
    contexts: &'static [Context],
    hint: &'static str,
    shown: Shown,
}

const EVERYWHERE: &[Context] = &[
    Context::Splash,
    Context::Home,
    Context::Main,
    Context::Help,
    Context::Picker,
    Context::Prompt,
    Context::Ask,
    Context::Palette,
];
const MAIN: &[Context] = &[Context::Main];

static DEFAULTS: &[Row] = &[
    Row {
        action: Action::Send,
        chords: &[
            Chord::plain(KeyCode::F(5)),
            Chord::new(KeyCode::Enter, Mods::CTRL),
            Chord::new(KeyCode::Enter, Mods::ALT),
        ],
        contexts: MAIN,
        hint: "send",
        shown: Shown::Bar,
    },
    Row {
        action: Action::NextPane,
        chords: &[Chord::plain(KeyCode::Tab)],
        contexts: MAIN,
        hint: "pane",
        shown: Shown::Bar,
    },
    Row {
        action: Action::Palette,
        chords: &[Chord::ctrl('k')],
        contexts: MAIN,
        hint: "palette",
        shown: Shown::Bar,
    },
    Row {
        action: Action::PickEnv,
        chords: &[Chord::plain(KeyCode::Char('e'))],
        contexts: MAIN,
        hint: "env",
        shown: Shown::Bar,
    },
    Row {
        action: Action::Trace,
        chords: &[Chord::plain(KeyCode::Char('t'))],
        contexts: MAIN,
        hint: "trace",
        shown: Shown::Bar,
    },
    Row {
        action: Action::Confirm,
        chords: &[Chord::plain(KeyCode::Enter)],
        contexts: &[
            Context::Picker,
            Context::Prompt,
            Context::Ask,
            Context::Palette,
        ],
        hint: "choose",
        shown: Shown::Bar,
    },
    Row {
        action: Action::Confirm,
        chords: &[Chord::plain(KeyCode::Enter)],
        contexts: MAIN,
        hint: "open or close folder",
        shown: Shown::HelpOnly,
    },
    Row {
        action: Action::Confirm,
        chords: &[Chord::plain(KeyCode::Char('y'))],
        contexts: &[Context::Ask],
        hint: "yes",
        shown: Shown::HelpOnly,
    },
    Row {
        action: Action::Cancel,
        chords: &[Chord::plain(KeyCode::Esc)],
        contexts: MAIN,
        hint: "cancel",
        shown: Shown::Bar,
    },
    Row {
        action: Action::PrevPane,
        chords: &[Chord::plain(KeyCode::BackTab)],
        contexts: MAIN,
        hint: "previous pane",
        shown: Shown::HelpOnly,
    },
    Row {
        action: Action::Up,
        chords: &[Chord::plain(KeyCode::Up), Chord::plain(KeyCode::Char('k'))],
        contexts: &[
            Context::Main,
            Context::Picker,
            Context::Palette,
            Context::Help,
        ],
        hint: "up",
        shown: Shown::HelpOnly,
    },
    Row {
        action: Action::Down,
        chords: &[
            Chord::plain(KeyCode::Down),
            Chord::plain(KeyCode::Char('j')),
        ],
        contexts: &[
            Context::Main,
            Context::Picker,
            Context::Palette,
            Context::Help,
        ],
        hint: "down",
        shown: Shown::HelpOnly,
    },
    Row {
        action: Action::PageUp,
        chords: &[Chord::plain(KeyCode::PageUp)],
        contexts: &[Context::Main, Context::Help],
        hint: "page up",
        shown: Shown::HelpOnly,
    },
    Row {
        action: Action::PageDown,
        chords: &[Chord::plain(KeyCode::PageDown)],
        contexts: &[Context::Main, Context::Help],
        hint: "page down",
        shown: Shown::HelpOnly,
    },
    Row {
        action: Action::Expand,
        chords: &[
            Chord::plain(KeyCode::Right),
            Chord::plain(KeyCode::Char('l')),
        ],
        contexts: MAIN,
        hint: "open folder",
        shown: Shown::HelpOnly,
    },
    Row {
        action: Action::Collapse,
        chords: &[
            Chord::plain(KeyCode::Left),
            Chord::plain(KeyCode::Char('h')),
        ],
        contexts: MAIN,
        hint: "close folder",
        shown: Shown::HelpOnly,
    },
    Row {
        action: Action::NewRequest,
        chords: &[Chord::plain(KeyCode::Char('a'))],
        contexts: MAIN,
        hint: "new request",
        shown: Shown::HelpOnly,
    },
    Row {
        action: Action::NewFolder,
        chords: &[Chord::plain(KeyCode::Char('A'))],
        contexts: MAIN,
        hint: "new folder",
        shown: Shown::HelpOnly,
    },
    Row {
        action: Action::Rename,
        chords: &[
            Chord::plain(KeyCode::Char('r')),
            Chord::plain(KeyCode::F(2)),
        ],
        contexts: MAIN,
        hint: "rename",
        shown: Shown::HelpOnly,
    },
    Row {
        action: Action::Delete,
        chords: &[
            Chord::plain(KeyCode::Char('d')),
            Chord::plain(KeyCode::Delete),
        ],
        contexts: MAIN,
        hint: "delete",
        shown: Shown::HelpOnly,
    },
    Row {
        action: Action::MoveUp,
        chords: &[Chord::plain(KeyCode::Char('K'))],
        contexts: MAIN,
        hint: "move up",
        shown: Shown::HelpOnly,
    },
    Row {
        action: Action::MoveDown,
        chords: &[Chord::plain(KeyCode::Char('J'))],
        contexts: MAIN,
        hint: "move down",
        shown: Shown::HelpOnly,
    },
    Row {
        action: Action::Reload,
        chords: &[Chord::ctrl('r')],
        contexts: MAIN,
        hint: "reload from disk",
        shown: Shown::HelpOnly,
    },
    Row {
        action: Action::Quit,
        chords: &[Chord::ctrl('q')],
        contexts: EVERYWHERE,
        hint: "quit",
        shown: Shown::Bar,
    },
    Row {
        action: Action::Help,
        chords: &[Chord::plain(KeyCode::F(1))],
        contexts: &[Context::Home, Context::Main],
        hint: "help",
        shown: Shown::Bar,
    },
    Row {
        action: Action::Close,
        chords: &[Chord::plain(KeyCode::Esc), Chord::plain(KeyCode::F(1))],
        contexts: &[
            Context::Help,
            Context::Picker,
            Context::Prompt,
            Context::Ask,
            Context::Palette,
        ],
        hint: "close",
        shown: Shown::Bar,
    },
    Row {
        action: Action::Close,
        chords: &[Chord::plain(KeyCode::Char('n'))],
        contexts: &[Context::Ask],
        hint: "no",
        shown: Shown::HelpOnly,
    },
    Row {
        action: Action::Copy,
        chords: &[Chord::ctrl('c')],
        contexts: &[Context::Home, Context::Main],
        hint: "copy the selection",
        shown: Shown::HelpOnly,
    },
];

/// The bindings in use.
#[derive(Debug, Clone)]
pub struct Keymap {
    bindings: Vec<Binding>,
}

impl Default for Keymap {
    fn default() -> Self {
        Self {
            bindings: DEFAULTS
                .iter()
                .map(|row| Binding {
                    action: row.action,
                    chords: row.chords.to_vec(),
                    contexts: row.contexts,
                    hint: row.hint,
                    shown: row.shown,
                })
                .collect(),
        }
    }
}

impl Keymap {
    /// Every binding, in table order.
    #[must_use]
    pub fn bindings(&self) -> &[Binding] {
        &self.bindings
    }

    /// The action bound to `chord` in `context`, with layout folding.
    #[must_use]
    pub fn lookup(&self, context: Context, chord: Chord) -> Option<Action> {
        let folded = match chord.code() {
            KeyCode::Char(c) => Chord::new(KeyCode::Char(fold(c)), chord.mods()),
            _ => chord,
        };
        self.bindings
            .iter()
            .find(|b| {
                b.contexts.contains(&context)
                    && b.chords.iter().any(|c| *c == chord || *c == folded)
            })
            .map(Binding::action)
    }

    /// `(key label, hint)` pairs for the hint bar in `context`.
    #[must_use]
    pub fn hints(&self, context: Context) -> Vec<(String, &'static str)> {
        self.bindings
            .iter()
            .filter(|b| b.in_bar() && b.contexts.contains(&context))
            .filter_map(|b| Some((b.chords.first()?.to_string(), b.hint)))
            .collect()
    }

    /// The first key of `action`.
    #[must_use]
    pub fn label(&self, action: Action) -> Option<String> {
        self.bindings
            .iter()
            .filter(|b| b.action == action)
            .find_map(|b| b.chords.first())
            .map(ToString::to_string)
    }

    /// The defaults with the overrides in `text` (a `keys.toml`) applied.
    ///
    /// # Errors
    ///
    /// `core::keys_invalid` for an unknown action or key, a bad chord, quit or help without a key,
    /// or one chord on two actions in the same context.
    pub fn parse(text: &str, path: &Path) -> Result<Self, CoreError> {
        let invalid = |reason: String| CoreError::keys_invalid(path, reason);
        let file: File = toml::from_str(text).map_err(|e| invalid(e.message().to_owned()))?;
        let mut keymap = Self::default();
        for (name, labels) in file.keys.unwrap_or_default() {
            let action: Action = name.parse().map_err(invalid)?;
            let chords = labels
                .iter()
                .map(|l| l.parse::<Chord>())
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| invalid(format!("`{name}`: {e}")))?;
            if chords.is_empty() && matches!(action, Action::Quit | Action::Help) {
                return Err(invalid(format!("`{name}` needs a key")));
            }
            for binding in keymap.bindings.iter_mut().filter(|b| b.action == action) {
                binding.chords.clone_from(&chords);
            }
        }
        keymap.check().map_err(invalid)?;
        Ok(keymap)
    }

    /// One chord on two actions that share a context.
    fn check(&self) -> Result<(), String> {
        for (i, a) in self.bindings.iter().enumerate() {
            for b in self.bindings.iter().skip(i + 1) {
                let shared = a.action != b.action
                    && a.contexts.iter().any(|c| b.contexts.contains(c))
                    && a.chords.iter().any(|c| b.chords.contains(c));
                if shared {
                    return Err(format!(
                        "`{}` and `{}` share a key",
                        a.action.name(),
                        b.action.name()
                    ));
                }
            }
        }
        Ok(())
    }

    /// Reads `keys.toml` at `path`: the defaults when it is missing; the defaults and the error
    /// when it is invalid.
    #[must_use]
    pub fn load(path: &Path) -> (Self, Option<CoreError>) {
        let text = match std::fs::read_to_string(path) {
            Ok(text) => text,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                return (Self::default(), None);
            }
            Err(err) => {
                return (
                    Self::default(),
                    Some(CoreError::keys_invalid(path, err.to_string())),
                );
            }
        };
        match Self::parse(&text, path) {
            Ok(keymap) => {
                tracing::info!(path = %path.display(), "keys.toml applied");
                (keymap, None)
            }
            Err(err) => {
                tracing::warn!(%err, "keys.toml ignored");
                (Self::default(), Some(err))
            }
        }
    }
}

/// `keys.toml`.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    keys: Option<std::collections::BTreeMap<String, Vec<String>>>,
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use std::path::Path;

    use pawsman_error::ErrorInfo as _;

    use super::*;
    use crate::keys::{Action, Context, KeyCode, Mods, fold};

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
        assert_eq!(
            Keymap::default().lookup(context, chord),
            expected,
            "{chord:?} in {context:?}"
        );
    }

    #[test]
    fn cyrillic_ctrl_q_quits() {
        let chord = Chord::new(KeyCode::Char('й'), Mods::CTRL);

        assert_eq!(
            Keymap::default().lookup(Context::Home, chord),
            Some(Action::Quit),
            "Ctrl-й is Ctrl-Q"
        );
    }

    #[test]
    fn shifted_cyrillic_folds_to_the_shifted_us_letter() {
        assert_eq!(fold('Й'), 'Q', "capital folds to capital");
        assert_eq!(fold('й'), 'q', "lowercase");
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
        let hints = Keymap::default().hints(Context::Home);

        assert_eq!(
            hints,
            vec![("Ctrl-Q".to_owned(), "quit"), ("F1".to_owned(), "help")],
            "bar hints on home"
        );
    }

    #[test]
    fn every_binding_has_keys_and_a_hint() {
        for binding in Keymap::default().bindings() {
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
            Keymap::default()
                .bindings()
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
    #[case::e_picks_env(Chord::plain(KeyCode::Char('e')), Action::PickEnv)]
    #[case::cyrillic_e(Chord::plain(KeyCode::Char('у')), Action::PickEnv)]
    #[case::t_trace(Chord::plain(KeyCode::Char('t')), Action::Trace)]
    #[case::cyrillic_t(Chord::plain(KeyCode::Char('е')), Action::Trace)]
    fn main_view_bindings(#[case] chord: Chord, #[case] expected: Action) {
        assert_eq!(
            Keymap::default().lookup(Context::Main, chord),
            Some(expected),
            "{chord}"
        );
    }

    #[test]
    fn main_hints_lead_with_send() {
        let hints = Keymap::default().hints(Context::Main);

        let keys: Vec<&str> = hints.iter().map(|(k, _)| k.as_str()).collect();
        assert_eq!(
            keys,
            ["F5", "Tab", "Ctrl-K", "e", "t", "Esc", "Ctrl-Q", "F1"],
            "{hints:?}"
        );
    }

    #[rstest]
    #[case::up(Chord::plain(KeyCode::Up), Some(Action::Up))]
    #[case::k(Chord::plain(KeyCode::Char('k')), Some(Action::Up))]
    #[case::down(Chord::plain(KeyCode::Down), Some(Action::Down))]
    #[case::j(Chord::plain(KeyCode::Char('j')), Some(Action::Down))]
    #[case::enter(Chord::plain(KeyCode::Enter), Some(Action::Confirm))]
    #[case::esc(Chord::plain(KeyCode::Esc), Some(Action::Close))]
    #[case::no_send(Chord::plain(KeyCode::F(5)), None)]
    fn picker_bindings(#[case] chord: Chord, #[case] expected: Option<Action>) {
        assert_eq!(
            Keymap::default().lookup(Context::Picker, chord),
            expected,
            "{chord}"
        );
    }

    #[test]
    fn main_keys_do_nothing_on_home() {
        assert_eq!(
            Keymap::default().lookup(Context::Home, Chord::plain(KeyCode::F(5))),
            None,
            "no send without a collection"
        );
    }

    #[test]
    fn every_context_can_quit() {
        for context in [
            Context::Splash,
            Context::Home,
            Context::Main,
            Context::Help,
            Context::Picker,
            Context::Prompt,
            Context::Ask,
            Context::Palette,
        ] {
            assert_eq!(
                Keymap::default().lookup(context, Chord::ctrl('q')),
                Some(Action::Quit),
                "{context:?}"
            );
        }
    }

    #[rstest]
    #[case::palette(Chord::ctrl('k'), Action::Palette)]
    #[case::reload(Chord::ctrl('r'), Action::Reload)]
    #[case::new_request(Chord::plain(KeyCode::Char('a')), Action::NewRequest)]
    #[case::cyrillic_new_request(Chord::plain(KeyCode::Char('ф')), Action::NewRequest)]
    #[case::new_folder(Chord::plain(KeyCode::Char('A')), Action::NewFolder)]
    #[case::rename(Chord::plain(KeyCode::Char('r')), Action::Rename)]
    #[case::f2(Chord::plain(KeyCode::F(2)), Action::Rename)]
    #[case::delete(Chord::plain(KeyCode::Char('d')), Action::Delete)]
    #[case::delete_key(Chord::plain(KeyCode::Delete), Action::Delete)]
    #[case::move_up(Chord::plain(KeyCode::Char('K')), Action::MoveUp)]
    #[case::cyrillic_move_up(Chord::plain(KeyCode::Char('Л')), Action::MoveUp)]
    #[case::move_down(Chord::plain(KeyCode::Char('J')), Action::MoveDown)]
    #[case::expand(Chord::plain(KeyCode::Right), Action::Expand)]
    #[case::l(Chord::plain(KeyCode::Char('l')), Action::Expand)]
    #[case::collapse(Chord::plain(KeyCode::Left), Action::Collapse)]
    #[case::h(Chord::plain(KeyCode::Char('h')), Action::Collapse)]
    #[case::enter_toggles(Chord::plain(KeyCode::Enter), Action::Confirm)]
    fn new_main_bindings(#[case] chord: Chord, #[case] expected: Action) {
        assert_eq!(
            Keymap::default().lookup(Context::Main, chord),
            Some(expected),
            "{chord}"
        );
    }

    #[rstest]
    #[case(Context::Prompt, Chord::plain(KeyCode::Enter), Some(Action::Confirm))]
    #[case(Context::Prompt, Chord::plain(KeyCode::Esc), Some(Action::Close))]
    #[case(Context::Ask, Chord::plain(KeyCode::Char('y')), Some(Action::Confirm))]
    #[case(Context::Ask, Chord::plain(KeyCode::Char('n')), Some(Action::Close))]
    #[case(Context::Ask, Chord::plain(KeyCode::Enter), Some(Action::Confirm))]
    #[case(Context::Palette, Chord::plain(KeyCode::Down), Some(Action::Down))]
    #[case(Context::Palette, Chord::plain(KeyCode::Enter), Some(Action::Confirm))]
    #[case(Context::Palette, Chord::plain(KeyCode::Esc), Some(Action::Close))]
    #[case(Context::Prompt, Chord::plain(KeyCode::F(5)), None)]
    fn overlay_bindings(
        #[case] context: Context,
        #[case] chord: Chord,
        #[case] expected: Option<Action>,
    ) {
        assert_eq!(
            Keymap::default().lookup(context, chord),
            expected,
            "{chord} in {context:?}"
        );
    }

    #[test]
    fn the_defaults_have_no_conflicts() {
        let text = "[keys]\n";

        assert!(Keymap::parse(text, Path::new("keys.toml")).is_ok(), "clean");
    }

    #[test]
    fn every_default_label_parses_back() {
        for binding in Keymap::default().bindings() {
            for chord in binding.chords() {
                assert_eq!(chord.to_string().parse::<Chord>(), Ok(*chord), "{chord}");
            }
        }
    }

    #[rstest]
    #[case("Ctrl-k", Chord::ctrl('k'))]
    #[case("ctrl-shift-K", Chord::new(KeyCode::Char('k'), Mods::CTRL | Mods::SHIFT))]
    #[case("PageDown", Chord::plain(KeyCode::PageDown))]
    #[case("Delete", Chord::plain(KeyCode::Delete))]
    #[case("Shift-a", Chord::plain(KeyCode::Char('A')))]
    #[case("Space", Chord::plain(KeyCode::Char(' ')))]
    #[case("F12", Chord::plain(KeyCode::F(12)))]
    #[case("Meta-x", Chord::new(KeyCode::Char('x'), Mods::META))]
    fn chord_aliases_parse(#[case] text: &str, #[case] expected: Chord) {
        assert_eq!(text.parse::<Chord>(), Ok(expected), "{text}");
    }

    #[rstest]
    #[case("")]
    #[case("Ctrl-")]
    #[case("Foo-K")]
    #[case("F0")]
    #[case("F25")]
    #[case("ab")]
    fn chord_parse_errors(#[case] text: &str) {
        assert!(text.parse::<Chord>().is_err(), "{text:?}");
    }

    fn parse(text: &str) -> Result<Keymap, crate::CoreError> {
        Keymap::parse(text, Path::new("/c/keys.toml"))
    }

    #[test]
    fn overrides_replace_an_actions_keys() {
        let keys = parse("[keys]\nsend = [\"F6\"]\n").unwrap();

        assert_eq!(
            keys.lookup(Context::Main, Chord::plain(KeyCode::F(6))),
            Some(Action::Send),
            "F6 sends"
        );
        assert_eq!(
            keys.lookup(Context::Main, Chord::plain(KeyCode::F(5))),
            None,
            "F5 no longer"
        );
        assert_eq!(keys.label(Action::Send).as_deref(), Some("F6"), "label");
    }

    #[test]
    fn an_unknown_action_is_keys_invalid() {
        let err = parse("[keys]\nlaunch = [\"F6\"]\n").unwrap_err();

        assert_eq!(err.code(), crate::codes::KEYS_INVALID, "code");
        assert!(err.to_string().contains("launch"), "{err}");
    }

    #[test]
    fn a_bad_chord_or_unknown_key_is_keys_invalid() {
        for text in [
            "[keys]\nsend = [\"Hyperdrive-X\"]\n",
            "[colors]\n",
            "keys = 1\n",
        ] {
            assert_eq!(
                parse(text).unwrap_err().code(),
                crate::codes::KEYS_INVALID,
                "{text}"
            );
        }
    }

    #[test]
    fn a_conflict_is_keys_invalid_and_names_both() {
        let err = parse("[keys]\ntrace = [\"e\"]\n").unwrap_err();

        assert_eq!(err.code(), crate::codes::KEYS_INVALID, "code");
        assert!(
            err.to_string().contains("trace") && err.to_string().contains("pick_env"),
            "{err}"
        );
    }

    #[test]
    fn quit_and_help_cannot_be_unbound() {
        for text in ["[keys]\nquit = []\n", "[keys]\nhelp = []\n"] {
            assert_eq!(
                parse(text).unwrap_err().code(),
                crate::codes::KEYS_INVALID,
                "{text}"
            );
        }
        let rebound = parse("[keys]\nquit = [\"Ctrl-X\"]\n").unwrap();
        assert_eq!(
            rebound.lookup(Context::Main, Chord::ctrl('x')),
            Some(Action::Quit),
            "quit may move"
        );
    }

    #[test]
    fn other_actions_can_be_unbound() {
        let keys = parse("[keys]\ntrace = []\n").unwrap();

        assert_eq!(
            keys.lookup(Context::Main, Chord::plain(KeyCode::Char('t'))),
            None
        );
        assert!(
            !keys.hints(Context::Main).iter().any(|(_, h)| *h == "trace"),
            "no hint without a key"
        );
    }

    #[test]
    fn a_missing_file_is_the_defaults() {
        let dir = tempfile::tempdir().unwrap();

        let (keys, err) = Keymap::load(&dir.path().join("keys.toml"));

        assert!(err.is_none(), "no error");
        assert_eq!(
            keys.lookup(Context::Main, Chord::plain(KeyCode::F(5))),
            Some(Action::Send)
        );
    }

    #[test]
    fn an_invalid_file_loads_the_defaults_and_the_error() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("keys.toml");
        std::fs::write(&path, "[keys]\nlaunch = []\n").unwrap();

        let (keys, err) = Keymap::load(&path);

        assert_eq!(err.map(|e| e.code()), Some(crate::codes::KEYS_INVALID));
        assert_eq!(
            keys.lookup(Context::Main, Chord::plain(KeyCode::F(5))),
            Some(Action::Send),
            "defaults"
        );
    }
}
