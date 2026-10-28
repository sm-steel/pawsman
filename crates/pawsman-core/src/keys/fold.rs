//! Layout folding: a key on a Cyrillic ЙЦУКЕН layout acts as the US key in the same place, so
//! commands work without switching layouts. Applied to commands only, never to typed text.

const CYRILLIC: &str = "йцукенгшщзхъфывапролджэячсмитьбю";
const US: &str = "qwertyuiop[]asdfghjkl;'zxcvbnm,.";

/// The US key in the same place as `c`, or `c` itself; a capital letter folds to a capital.
#[must_use]
pub fn fold(c: char) -> char {
    let lower = c.to_lowercase().next().unwrap_or(c);
    if lower == 'ё' {
        return '`';
    }
    let Some(us) = CYRILLIC
        .chars()
        .zip(US.chars())
        .find(|(cyr, _)| *cyr == lower)
        .map(|(_, us)| us)
    else {
        return c;
    };
    if c.is_uppercase() {
        us.to_ascii_uppercase()
    } else {
        us
    }
}
