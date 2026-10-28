//! What the command palette offers: actions, then every request, ranked by the fuzzy matcher.

use pawsman_core::fuzzy::{self, Match};
use pawsman_core::keys::{Action, Context, Keymap};
use pawsman_core::state::Snapshot;
use pawsman_model::{EntryKind, RequestId};

/// One palette row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Item {
    /// Select this request.
    Request {
        /// The request.
        id: RequestId,
        /// The method, `None` for a file that does not parse.
        method: Option<String>,
        /// Its name and folder, or its file for an invalid one.
        label: String,
    },
    /// Run this action.
    Action {
        /// The action.
        action: Action,
        /// The capitalised hint.
        label: String,
        /// Its first key.
        key: Option<String>,
    },
}

impl Item {
    /// The text the query matches.
    pub(crate) fn label(&self) -> &str {
        match self {
            Self::Request { label, .. } | Self::Action { label, .. } => label,
        }
    }
}

/// Actions that only make sense as keys: moving and choosing.
const NOT_IN_PALETTE: &[Action] = &[
    Action::Up,
    Action::Down,
    Action::PageUp,
    Action::PageDown,
    Action::Confirm,
    Action::Expand,
    Action::Collapse,
    Action::Palette,
];

/// The main view's actions, then every request in tree order.
pub(crate) fn items(snapshot: &Snapshot, keys: &Keymap) -> Vec<Item> {
    let mut items: Vec<Item> = Vec::new();
    for binding in keys.bindings() {
        let action = binding.action();
        let listed = items
            .iter()
            .any(|i| matches!(i, Item::Action { action: a, .. } if *a == action));
        if listed
            || !binding.contexts().contains(&Context::Main)
            || NOT_IN_PALETTE.contains(&action)
        {
            continue;
        }
        items.push(Item::Action {
            action,
            label: title(action).map_or_else(|| capitalised(binding.hint()), str::to_owned),
            key: keys.label(action),
        });
    }
    let Some(collection) = snapshot.collection() else {
        return items;
    };
    items.extend(collection.entries().iter().map(|entry| match entry.kind() {
        EntryKind::Request(request) => {
            let folder = entry.folder().display().to_string();
            let label = if folder.is_empty() {
                request.name().to_owned()
            } else {
                format!("{}  {folder}", request.name())
            };
            Item::Request {
                id: entry.id(),
                method: Some(request.method().to_owned()),
                label,
            }
        }
        _ => Item::Request {
            id: entry.id(),
            method: None,
            label: entry.path().display().to_string(),
        },
    }));
    items
}

/// The items matching `query`, best first.
pub(crate) fn filter(items: Vec<Item>, query: &str) -> Vec<(Item, Match)> {
    fuzzy::rank(
        query,
        items.into_iter().map(|item| {
            let label = item.label().to_owned();
            (item, label)
        }),
    )
}

/// A longer name where the hint bar's word is too short to search for.
fn title(action: Action) -> Option<&'static str> {
    Some(match action {
        Action::Send => "Send request",
        Action::Cancel => "Cancel request",
        Action::NextPane => "Next pane",
        Action::PrevPane => "Previous pane",
        Action::PickEnv => "Pick environment",
        Action::Trace => "Show or hide the variable trace",
        Action::Help => "Help: all keys",
        Action::Quit => "Quit pawsman",
        _ => return None,
    })
}

fn capitalised(hint: &str) -> String {
    let mut chars = hint.chars();
    chars.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(chars).collect()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hints_are_capitalised() {
        assert_eq!(capitalised("pick env"), "Pick env");
        assert_eq!(capitalised(""), "");
    }
}
