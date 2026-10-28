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

/// Points for each matched character.
const MATCH: i64 = 16;
/// Extra for a match at the start of the text.
const AT_START: i64 = 24;
/// Extra for a match after a separator or at a lower-to-upper change.
const AT_WORD: i64 = 20;
/// Extra for a match right after the previous one.
const IN_A_ROW: i64 = 20;
/// The cost of a gap: this much for the first skipped character…
const GAP_OPEN: i64 = 6;
/// …and one more per further skipped character, up to this many.
const GAP_GROWS: usize = 4;
/// The largest gap cost.
const GAP_MAX: i64 = GAP_OPEN + 4;
const UNREACHED: i64 = i64::MIN / 4;

/// How `text` matches `query`, ignoring case and spaces in the query; `None` when it does not.
#[must_use]
pub fn score(query: &str, text: &str) -> Option<Match> {
    let query: Vec<char> = query
        .chars()
        .filter(|c| !c.is_whitespace())
        .map(lower)
        .collect();
    if query.is_empty() {
        return Some(Match {
            score: 0,
            positions: Vec::new(),
        });
    }
    let original: Vec<char> = text.chars().collect();
    let text: Vec<char> = original.iter().copied().map(lower).collect();
    let mut rest = text.iter();
    if !query.iter().all(|q| rest.any(|t| t == q)) {
        return None;
    }
    // best[i][j]: the best score with query[..=i] matched and query[i] at text[j];
    // from[i][j]: where query[i - 1] sat on that path.
    let mut best = vec![vec![UNREACHED; text.len()]; query.len()];
    let mut from = vec![vec![0usize; text.len()]; query.len()];
    for (j, t) in text.iter().enumerate() {
        if Some(t) == query.first()
            && let Some(cell) = best.first_mut().and_then(|row| row.get_mut(j))
        {
            *cell = MATCH + bonus(&original, j);
        }
    }
    for i in 1..query.len() {
        let (done, todo) = best.split_at_mut(i);
        let (Some(prev), Some(row), Some(q)) = (done.last(), todo.first_mut(), query.get(i)) else {
            break;
        };
        // The best previous score far enough back to pay the largest gap cost.
        let mut far = (UNREACHED, 0);
        for j in 1..text.len() {
            if let Some(k) = j.checked_sub(GAP_GROWS + 2)
                && let Some(&s) = prev.get(k)
                && s > far.0
            {
                far = (s, k);
            }
            if text.get(j) != Some(q) {
                continue;
            }
            let mut pick = (far.0 - GAP_MAX, far.1);
            for k in j.saturating_sub(GAP_GROWS + 1)..j {
                let Some(&s) = prev.get(k) else {
                    continue;
                };
                let skipped = j - k - 1;
                let step = if skipped == 0 {
                    IN_A_ROW
                } else {
                    -(GAP_OPEN + i64::try_from(skipped - 1).unwrap_or(0))
                };
                if s + step > pick.0 {
                    pick = (s + step, k);
                }
            }
            if pick.0 > UNREACHED / 2
                && let (Some(cell), Some(back)) =
                    (row.get_mut(j), from.get_mut(i).and_then(|r| r.get_mut(j)))
            {
                *cell = pick.0 + MATCH + bonus(&original, j);
                *back = pick.1;
            }
        }
    }
    let last = best.last()?;
    let (mut j, total) = last.iter().copied().enumerate().fold(
        None,
        |acc: Option<(usize, i64)>, (j, s)| match acc {
            Some((_, b)) if b >= s => acc,
            _ if s > UNREACHED / 2 => Some((j, s)),
            _ => acc,
        },
    )?;
    let mut positions = vec![j];
    for i in (1..query.len()).rev() {
        j = *from.get(i)?.get(j)?;
        positions.push(j);
    }
    positions.reverse();
    Some(Match {
        score: u32::try_from(total.max(0)).unwrap_or(u32::MAX),
        positions,
    })
}

/// The items matching `query`, best first; equal scores keep their order.
pub fn rank<T>(query: &str, items: impl IntoIterator<Item = (T, String)>) -> Vec<(T, Match)> {
    let mut ranked: Vec<(T, Match)> = items
        .into_iter()
        .filter_map(|(item, text)| score(query, &text).map(|m| (item, m)))
        .collect();
    ranked.sort_by_key(|(_, m)| std::cmp::Reverse(m.score));
    ranked
}

fn lower(c: char) -> char {
    c.to_lowercase().next().unwrap_or(c)
}

/// Extra points for a match at char `j`: the start, after a separator, or a case change.
fn bonus(text: &[char], j: usize) -> i64 {
    let Some(before) = j.checked_sub(1).and_then(|k| text.get(k)) else {
        return AT_START;
    };
    let here = text.get(j).copied().unwrap_or(' ');
    if matches!(before, ' ' | '-' | '_' | '.' | '/')
        || (before.is_lowercase() && here.is_uppercase())
    {
        AT_WORD
    } else {
        0
    }
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

    fn points(query: &str, text: &str) -> u32 {
        score(query, text).unwrap().score()
    }

    #[test]
    fn scores_follow_the_formula() {
        assert_eq!(
            points("ab", "ab"),
            16 + 24 + 16 + 20,
            "start, then in a row"
        );
        assert_eq!(
            points("ab", "a-b"),
            40 - 6 + 16 + 20,
            "one skipped, word start"
        );
        assert_eq!(points("ab", "axxb"), 40 - 7 + 16, "two skipped");
        assert_eq!(points("ab", "axxxxb"), 40 - 9 + 16, "four skipped");
        assert_eq!(
            points("ab", "axxxxxb"),
            40 - 10 + 16,
            "five skipped: the cap"
        );
        assert_eq!(
            points("ab", "axxxxxxxxxb"),
            40 - 10 + 16,
            "nine skipped: still the cap"
        );
        assert_eq!(
            points("gU", "getUser"),
            40 - 7 + 16 + 20,
            "camel case starts a word"
        );
        assert_eq!(points("x", "yx"), 16, "a plain middle");
    }

    #[test]
    fn the_best_alignment_wins_not_the_first() {
        let m = score("ab", "a_xb_ab").unwrap();

        assert_eq!(
            m.positions(),
            [5, 6],
            "word start and in a row beat the early a"
        );
        assert_eq!(m.score(), 16 + 20 + 16 + 20);
    }

    #[test]
    fn equal_scores_keep_the_earliest_match() {
        assert_eq!(score("x", "-x-x").unwrap().positions(), [1]);
        assert_eq!(order("a", &["xa", "ya"]), ["xa", "ya"], "stable");
    }
}
