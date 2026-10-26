//! The process environment, read once at start (OBSERVABILITY P1).

use std::collections::BTreeMap;

/// The process environment as it was at start. Nothing else reads `std::env`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EnvSnapshot {
    vars: BTreeMap<String, String>,
}

impl EnvSnapshot {
    /// Reads the process environment; variables that are not UTF-8 are skipped.
    #[must_use]
    #[expect(
        clippy::disallowed_methods,
        reason = "the one place that reads the process env (OBSERVABILITY P1)"
    )]
    pub fn capture() -> Self {
        let vars = std::env::vars_os()
            .filter_map(|(k, v)| Some((k.into_string().ok()?, v.into_string().ok()?)))
            .collect();
        Self { vars }
    }

    /// Builds a snapshot from pairs, for tests and child processes.
    #[must_use]
    pub fn from_pairs<I, K, V>(pairs: I) -> Self
    where
        I: IntoIterator<Item = (K, V)>,
        K: Into<String>,
        V: Into<String>,
    {
        Self {
            vars: pairs
                .into_iter()
                .map(|(k, v)| (k.into(), v.into()))
                .collect(),
        }
    }

    /// The value of `name`; an empty value counts as unset.
    #[must_use]
    pub fn var(&self, name: &str) -> Option<&str> {
        self.vars
            .get(name)
            .map(String::as_str)
            .filter(|v| !v.is_empty())
    }

    /// `TERM`.
    #[must_use]
    pub fn term(&self) -> Option<&str> {
        self.var("TERM")
    }

    /// Running inside tmux.
    #[must_use]
    pub fn in_tmux(&self) -> bool {
        self.var("TMUX").is_some()
    }

    /// Running in an SSH session.
    #[must_use]
    pub fn over_ssh(&self) -> bool {
        self.var("SSH_TTY").is_some() || self.var("SSH_CONNECTION").is_some()
    }

    /// The first set of `LC_ALL`, `LC_CTYPE`, `LANG` names UTF-8.
    #[must_use]
    pub fn utf8_locale(&self) -> bool {
        ["LC_ALL", "LC_CTYPE", "LANG"]
            .into_iter()
            .find_map(|name| self.var(name))
            .is_some_and(|locale| {
                let locale = locale.to_ascii_lowercase();
                locale.contains("utf-8") || locale.contains("utf8")
            })
    }

    /// `COLORTERM` says truecolor.
    #[must_use]
    pub fn truecolor(&self) -> bool {
        matches!(self.var("COLORTERM"), Some("truecolor" | "24bit"))
    }

    /// `PAWSMAN_NO_ANIMATION` is set to anything but `0`.
    #[must_use]
    pub fn no_animation(&self) -> bool {
        self.var("PAWSMAN_NO_ANIMATION").is_some_and(|v| v != "0")
    }

    /// The log filter: `PAWSMAN_LOG`, else `RUST_LOG`.
    #[must_use]
    pub fn log_filter(&self) -> Option<&str> {
        self.var("PAWSMAN_LOG").or_else(|| self.var("RUST_LOG"))
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    fn env(pairs: &[(&str, &str)]) -> EnvSnapshot {
        EnvSnapshot::from_pairs(pairs.iter().copied())
    }

    #[test]
    fn empty_values_count_as_unset() {
        let env = env(&[("TMUX", ""), ("TERM", "xterm-kitty")]);

        assert_eq!(env.var("TMUX"), None, "an empty TMUX is not tmux");
        assert_eq!(env.term(), Some("xterm-kitty"), "TERM is read");
    }

    #[rstest]
    #[case::lang(&[("LANG", "en_US.UTF-8")], true)]
    #[case::lowercase_utf8(&[("LANG", "C.utf8")], true)]
    #[case::lc_all_wins(&[("LC_ALL", "C"), ("LANG", "en_US.UTF-8")], false)]
    #[case::lc_ctype_before_lang(&[("LC_CTYPE", "ru_RU.UTF-8"), ("LANG", "C")], true)]
    #[case::empty_lc_all_skipped(&[("LC_ALL", ""), ("LANG", "C.UTF-8")], true)]
    #[case::none(&[], false)]
    fn utf8_locale_takes_the_first_set_variable(
        #[case] pairs: &[(&str, &str)],
        #[case] expected: bool,
    ) {
        let env = env(pairs);

        assert_eq!(env.utf8_locale(), expected, "locale from {pairs:?}");
    }

    #[rstest]
    #[case::ssh_tty(&[("SSH_TTY", "/dev/pts/3")], true)]
    #[case::ssh_connection(&[("SSH_CONNECTION", "10.0.0.1 5 10.0.0.2 22")], true)]
    #[case::local(&[], false)]
    fn over_ssh_from_either_variable(#[case] pairs: &[(&str, &str)], #[case] expected: bool) {
        assert_eq!(env(pairs).over_ssh(), expected, "ssh from {pairs:?}");
    }

    #[rstest]
    #[case::truecolor("truecolor", true)]
    #[case::bits("24bit", true)]
    #[case::other("yes", false)]
    fn truecolor_from_colorterm(#[case] value: &str, #[case] expected: bool) {
        assert_eq!(
            env(&[("COLORTERM", value)]).truecolor(),
            expected,
            "COLORTERM={value}"
        );
    }

    #[rstest]
    #[case::one("1", true)]
    #[case::word("yes", true)]
    #[case::zero("0", false)]
    #[case::empty("", false)]
    fn no_animation_unless_zero_or_empty(#[case] value: &str, #[case] expected: bool) {
        assert_eq!(
            env(&[("PAWSMAN_NO_ANIMATION", value)]).no_animation(),
            expected,
            "value {value:?}"
        );
    }

    #[test]
    fn pawsman_log_wins_over_rust_log() {
        let both = env(&[("PAWSMAN_LOG", "pawsman=debug"), ("RUST_LOG", "trace")]);
        let rust_only = env(&[("RUST_LOG", "trace")]);

        assert_eq!(
            both.log_filter(),
            Some("pawsman=debug"),
            "PAWSMAN_LOG first"
        );
        assert_eq!(
            rust_only.log_filter(),
            Some("trace"),
            "RUST_LOG as fallback"
        );
    }

    #[test]
    fn tmux_needs_a_value() {
        assert!(
            env(&[("TMUX", "/tmp/tmux-1000/default,1,0")]).in_tmux(),
            "TMUX set"
        );
        assert!(!env(&[]).in_tmux(), "TMUX unset");
    }
}
