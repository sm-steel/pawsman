//! Fuzzy matching for the command palette: every query character in order, scored so that word
//! starts and runs of adjacent characters rank first.

/// A match and where it hit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Match {
    score: u32,
    positions: Vec<usize>,
}

impl Match {
    /// Higher is better.
    #[must_use]
    pub fn score(&self) -> u32 {
        self.score
    }

    /// The char indices of the matched characters in the text, increasing.
    #[must_use]
    pub fn positions(&self) -> &[usize] {
        &self.positions
    }
}

/// How `text` matches `query`, ignoring case and spaces in the query; `None` when it does not.
#[must_use]
pub fn score(query: &str, text: &str) -> Option<Match> {
    let _ = (query, text);
    None
}

/// The items matching `query`, best first; equal scores keep their order.
pub fn rank<T>(query: &str, items: impl IntoIterator<Item = (T, String)>) -> Vec<(T, Match)> {
    let _ = query;
    items
        .into_iter()
        .take(0)
        .map(|(t, _)| {
            (
                t,
                Match {
                    score: 0,
                    positions: Vec::new(),
                },
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    fn order(query: &str, texts: &[&str]) -> Vec<String> {
        rank(query, texts.iter().map(|t| (t.to_string(), t.to_string())))
            .into_iter()
            .map(|(t, _)| t)
            .collect()
    }

    #[test]
    fn a_subsequence_is_required() {
        assert!(score("cu", "create user").is_some(), "in order");
        assert!(score("uc", "cu").is_none(), "out of order");
        assert!(score("x", "create user").is_none(), "missing");
    }

    #[test]
    fn word_starts_beat_middles() {
        assert_eq!(
            order("cu", &["accuracy", "create user"]),
            ["create user", "accuracy"]
        );
    }

    #[test]
    fn consecutive_beats_scattered() {
        assert_eq!(order("user", &["u-s-e-r", "users"]), ["users", "u-s-e-r"]);
    }

    #[test]
    fn case_and_query_spaces_are_ignored() {
        assert!(score("CrUs", "create user").is_some(), "case");
        assert!(
            score("cr us", "createuser").is_some(),
            "spaces in the query"
        );
    }

    #[test]
    fn cyrillic_matches() {
        let m = score("пол", "Пользователи").unwrap();

        assert_eq!(m.positions(), [0, 1, 2], "chars, not bytes");
    }

    #[test]
    fn empty_query_keeps_the_order() {
        assert_eq!(order("", &["b", "a", "c"]), ["b", "a", "c"]);
        assert_eq!(score("", "x").map(|m| m.score()), Some(0), "score 0");
    }

    #[test]
    fn positions_mark_the_matched_chars() {
        let m = score("gu", "Get user").unwrap();

        assert_eq!(m.positions(), [0, 4], "word starts preferred");
    }

    proptest! {
        #[test]
        fn positions_are_increasing_and_spell_the_query(
            text in "[a-zA-Zа-я _/-]{0,30}",
            picks in proptest::collection::vec(any::<bool>(), 30),
        ) {
            let chars: Vec<char> = text.chars().collect();
            let query: String = chars
                .iter()
                .zip(&picks)
                .filter(|(c, keep)| **keep && **c != ' ')
                .map(|(c, _)| *c)
                .collect();
            let m = score(&query, &text).expect("a subsequence always matches");
            prop_assert!(m.positions().windows(2).all(|w| w[0] < w[1]), "{:?}", m);
            let spelled: String = m
                .positions()
                .iter()
                .filter_map(|i| chars.get(*i))
                .flat_map(|c| c.to_lowercase())
                .collect();
            let wanted: String = query.chars().flat_map(char::to_lowercase).collect();
            prop_assert_eq!(spelled, wanted);
        }
    }
}
