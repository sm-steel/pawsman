//! The key table. The hint bar, the F1 overlay and dispatch all read the [`Keymap`];
//! `~/.config/pawsman/keys.toml` can rebind any action.

mod chord;
mod fold;
mod keymap;

use std::str::FromStr;

pub use chord::{Chord, KeyCode, Mods};
pub use fold::fold;
pub use keymap::{Binding, Keymap};

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
    /// Open the environment picker.
    PickEnv,
    /// Show or hide the variable trace.
    Trace,
    /// Choose the highlighted item.
    Confirm,
    /// Open the command palette.
    Palette,
    /// Read the collection from disk again.
    Reload,
    /// Create a request in the selected folder.
    NewRequest,
    /// Create a folder in the selected folder.
    NewFolder,
    /// Rename the selected node.
    Rename,
    /// Delete the selected node.
    Delete,
    /// Move the selected node up among its siblings.
    MoveUp,
    /// Move the selected node down among its siblings.
    MoveDown,
    /// Open the selected folder.
    Expand,
    /// Close the selected folder, or go to its parent.
    Collapse,
}

/// Every action with its name in `keys.toml`.
const NAMES: &[(Action, &str)] = &[
    (Action::Quit, "quit"),
    (Action::Help, "help"),
    (Action::Close, "close"),
    (Action::Copy, "copy"),
    (Action::Send, "send"),
    (Action::Cancel, "cancel"),
    (Action::NextPane, "next_pane"),
    (Action::PrevPane, "prev_pane"),
    (Action::Up, "up"),
    (Action::Down, "down"),
    (Action::PageUp, "page_up"),
    (Action::PageDown, "page_down"),
    (Action::PickEnv, "pick_env"),
    (Action::Trace, "trace"),
    (Action::Confirm, "confirm"),
    (Action::Palette, "palette"),
    (Action::Reload, "reload"),
    (Action::NewRequest, "new_request"),
    (Action::NewFolder, "new_folder"),
    (Action::Rename, "rename"),
    (Action::Delete, "delete"),
    (Action::MoveUp, "move_up"),
    (Action::MoveDown, "move_down"),
    (Action::Expand, "expand"),
    (Action::Collapse, "collapse"),
];

impl Action {
    /// The name `keys.toml` uses.
    #[must_use]
    pub fn name(self) -> &'static str {
        NAMES
            .iter()
            .find(|(a, _)| *a == self)
            .map_or("", |(_, n)| n)
    }
}

impl FromStr for Action {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        NAMES
            .iter()
            .find(|(_, n)| *n == s)
            .map(|(a, _)| *a)
            .ok_or_else(|| format!("unknown action `{s}`"))
    }
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
    /// The environment picker.
    Picker,
    /// A one-line text field, such as a new name.
    Prompt,
    /// A yes or no question.
    Ask,
    /// The command palette.
    Palette,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_action_has_a_unique_name_that_parses_back() {
        for (action, name) in NAMES {
            assert_eq!(action.name(), *name, "{action:?}");
            assert_eq!(name.parse::<Action>(), Ok(*action), "{name}");
        }
        let mut names: Vec<&str> = NAMES.iter().map(|(_, n)| *n).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), NAMES.len(), "unique");
        assert!("nope".parse::<Action>().is_err(), "unknown");
    }
}
