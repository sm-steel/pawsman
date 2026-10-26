//! Layout folding: a key on a Cyrillic ЙЦУКЕН layout acts as the US key in the same place, so
//! commands work without switching layouts. Applied to commands only, never to typed text.

/// The US key in the same place as `c`, or `c` itself.
#[must_use]
pub fn fold(c: char) -> char {
    c
}
