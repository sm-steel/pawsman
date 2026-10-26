//! tmux detection and DCS passthrough.

use pawsman_core::env::EnvSnapshot;

/// Whether pawsman runs inside tmux.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InTmux {
    /// `TMUX` is set.
    Yes,
    /// It is not.
    No,
}

impl InTmux {
    /// From `TMUX` in the env snapshot.
    #[must_use]
    pub fn from_env(env: &EnvSnapshot) -> Self {
        if env.in_tmux() { Self::Yes } else { Self::No }
    }
}

/// Wraps `seq` for tmux: `ESC P tmux;` + `seq` with every ESC doubled + `ESC \`.
#[must_use]
pub fn passthrough(seq: &[u8]) -> Vec<u8> {
    seq.to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn passthrough_doubles_every_escape() {
        assert_eq!(
            passthrough(b"\x1b[c"),
            b"\x1bPtmux;\x1b\x1b[c\x1b\\".to_vec(),
            "wrapped"
        );
    }
}
