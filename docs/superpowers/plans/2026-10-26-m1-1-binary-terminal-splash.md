# M1.1 Binary, terminal and splash: implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** a `pawsman` binary that probes the terminal, plays the startup splash, shows a placeholder home view with a hint bar and an F1 help overlay, and quits on Ctrl-Q leaving the terminal exactly as it found it, in kitty, over SSH, in tmux and in xterm.

**Architecture:** `pawsman-core` gets the env snapshot, paths, settings, notices and the first rows of the key table (no terminal deps). `pawsman-tui` owns `/dev/tty`: raw mode through `rustix::termios`, a global mode stack undone newest first, the capability probe (pure parser plus a poll-based reader), a pure input decoder fed by an input thread that also watches signal pipes, the splash and the home view, and a main loop that drains every queued message before one draw and only ticks while something time-based is on screen. `pawsman-cli` parses flags, installs the rolling file log, and prints one exit line after restore on error.

**Tech stack:** Rust 2024 (toolchain 1.99.0, MSRV 1.97), ratatui 0.30.2, crossterm 0.29.0 (backend only), tachyonfx 0.25.2, rustix 1.1.5, signal-hook 0.4.5, clap 4.6.7, tracing 0.1.44, tracing-subscriber 0.3.23, tracing-appender 0.2.5, serde 1.0.229, toml 1.1.8, unicode-width 0.2.2, unicode-segmentation 1.13.3; dev: insta 1.49.0, rstest 0.28.0, pty-process 0.5.4, vt100 0.18.1, fontdue 0.9.5, image 0.25.10, tempfile 3.27.0.

**Spec:** [M1 milestone design, piece 1.1](../specs/2026-10-26-m1-http-client-design.md#11-binary-terminal-and-splash), within the [program spec](../specs/2026-10-23-pawsman-design.md).

## Global constraints

- Rules in [rules/](../../../rules/) apply; [rust-project.md](../../../rules/rust-project.md) wins every conflict.
- No `unwrap`/`expect`/`panic!`/indexing outside tests; `#[expect(…, reason = "…")]`, never `#[allow]`; `#![forbid(unsafe_code)]` in every crate root; `///` on every public item.
- Every dependency pinned `"=x.y.z"` in `[workspace.dependencies]`; crates use `dep.workspace = true` and `[lints] workspace = true`.
- Only `pawsman-tui` links ratatui, crossterm or tachyonfx; `pawsman-core` never does (`tools/check-deps.sh`, `deny.toml`).
- Nothing reads the process env except `EnvSnapshot::capture`; nothing writes stdout or stderr except the exit line in `main`.
- Tests first: a `test:` commit failing on assertions, then a `feat:` commit. Approved tests are never weakened.
- Every new error code has a page under `docs/errors/<namespace>/<slug>.md`, checked by a registry test.
- Gates before each commit that touches Rust: `CARGO_BUILD_WARNINGS=deny cargo clippy --workspace --all-targets --locked`, `cargo nextest run --workspace`, `tools/check-deps.sh`; docs: `markdownlint-cli2 "**/*.md" "#target" "#node_modules" "#.superpowers"`, `typos`.
- Probe timings: 1 s for replies plus 150 ms grace; inside tmux at most 500 ms after tmux's own DA1.
- Mouse modes 1000, 1002, 1006 only; kitty keyboard flags `DISAMBIGUATE` (`CSI > 1 u`) only.
- Commit email is the repo-local `sm.steel@hotmail.com`; never `--no-verify`.

## Review focus

Inputs the spec implies but no task's main tests would exercise; each has its test in the owning task.

1. **A Russian keyboard layout.** Under the kitty protocol Ctrl-Q on a ЙЦУКЕН layout arrives as `CSI 1081;5u` (Ctrl-й); it must quit like Ctrl-Q (Task 3 `cyrillic_ctrl_q_quits`, Task 11 PTY `kitty_cyrillic_ctrl_q_quits`).
2. **Unbounded input.** A huge paste or a stray `ESC P` / `ESC _` with no terminator must not grow memory without bound or swallow every later key (Task 4 `unterminated_string_is_dropped_at_the_cap`, `paste_is_capped`).
3. **The SSH link drops** (SIGHUP) during the splash: modes restored, the process exits within a second (Task 11 PTY `sighup_restores_and_exits`).
4. **A tiny or zero-sized terminal** (0×0, 1×1, 10×3) at start or after a resize: no panic, the splash ends, home degrades (Task 10 `tiny_areas_never_panic`).
5. **Redirected stdout** (`pawsman > out.txt`): frames go to `/dev/tty`, the file stays empty (Task 11 PTY `redirected_stdout_stays_empty`).

---

## File map

| File | Responsibility |
| --- | --- |
| `Cargo.toml` | workspace deps for this piece |
| `mise.toml` | cargo-insta |
| `crates/pawsman-test-support/src/lib.rs`, `errors.rs` | error-page registry assertion |
| `crates/pawsman-core/src/lib.rs` | crate root |
| `crates/pawsman-core/src/env.rs` | `EnvSnapshot` |
| `crates/pawsman-core/src/paths.rs` | `Paths` (XDG with `$HOME` fallback) |
| `crates/pawsman-core/src/settings.rs` | `Settings`, `Ui`, `Animations`, `ImageMode` |
| `crates/pawsman-core/src/keys.rs`, `keys/chord.rs`, `keys/fold.rs` | key types, the table, layout folding |
| `crates/pawsman-core/src/notice.rs` | `Notice`, `Level` |
| `crates/pawsman-core/src/error.rs`, `codes.rs` | `CoreError` and codes |
| `crates/pawsman-tui/src/lib.rs` | crate root, `run`, `Options` |
| `crates/pawsman-tui/src/error.rs`, `codes.rs` | `TuiError` and codes |
| `crates/pawsman-tui/src/input.rs`, `input/decoder.rs`, `input/reader.rs` | `Input`, the decoder, the input thread |
| `crates/pawsman-tui/src/term.rs` | `Term`: setup, input modes, probe write, restore |
| `crates/pawsman-tui/src/term/modes.rs` | `Mode`, `MODES`, the global restore state, panic hook |
| `crates/pawsman-tui/src/term/tmux.rs` | `InTmux`, DCS passthrough |
| `crates/pawsman-tui/src/term/probe.rs` | query bytes, reply parser, reply reader |
| `crates/pawsman-tui/src/term/caps.rs` | `Caps` and its resolution |
| `crates/pawsman-tui/src/app.rs` | `Msg`, `App` state machine, the event loop |
| `crates/pawsman-tui/src/views.rs`, `views/colors.rs`, `views/splash.rs`, `views/home.rs`, `views/help.rs`, `views/hints.rs` | drawing |
| `crates/pawsman-tui/src/visual.rs`, `visual/png.rs` | `#[ignore]` PNG scenarios (test only) |
| `crates/pawsman-cli/src/main.rs`, `args.rs`, `log.rs` | binary |
| `crates/pawsman-cli/tests/it/main.rs`, `it/pty.rs`, `it/probe_paths.rs`, `it/lifecycle.rs` | PTY tests |
| `docs/errors/core/*.md`, `docs/errors/tui/*.md` | error pages |
| `deny.toml`, `tools/check-deps.sh` | boundaries |

---

### Task 1: `pawsman-core` with the env snapshot

**Files:**

- Modify: `Cargo.toml` (members are `crates/*` already; add deps)
- Create: `crates/pawsman-core/Cargo.toml`, `crates/pawsman-core/src/lib.rs`, `crates/pawsman-core/src/env.rs`

**Interfaces:**

- Produces: `pawsman_core::env::EnvSnapshot` with `capture()`, `from_pairs(I) where I: IntoIterator<Item = (K, V)>, K: Into<String>, V: Into<String>`, `var(&self, &str) -> Option<&str>` (empty counts as unset), `term() -> Option<&str>`, `in_tmux() -> bool`, `over_ssh() -> bool`, `utf8_locale() -> bool`, `truecolor() -> bool`, `no_animation() -> bool`, `log_filter() -> Option<&str>`. These return `bool` (queries, not parameters), which rule 3 allows.

- [ ] **Step 1: Add workspace deps and the crate**

In root `Cargo.toml` `[workspace.dependencies]` add (internal crates first, then external, alphabetical):

```toml
pawsman-core = { path = "crates/pawsman-core" }
pawsman-tui = { path = "crates/pawsman-tui" }
pawsman-test-support = { path = "crates/pawsman-test-support" }

clap = { version = "=4.6.7", features = ["derive"] }
crossterm = "=0.29.0"
fontdue = "=0.9.5"
image = { version = "=0.25.10", default-features = false, features = ["png"] }
insta = "=1.49.0"
pty-process = "=0.5.4"
ratatui = { version = "=0.30.2", default-features = false, features = ["crossterm"] }
rstest = { version = "=0.28.0", default-features = false }
rustix = { version = "=1.1.5", features = ["event", "fs", "termios", "process", "stdio"] }
serde = { version = "=1.0.229", features = ["derive"] }
signal-hook = { version = "=0.4.5", default-features = false }
tachyonfx = { version = "=0.25.2", default-features = false, features = ["std"] }
tempfile = "=3.27.0"
toml = { version = "=1.1.8", default-features = false, features = ["parse", "serde", "display"] }
tracing = "=0.1.44"
tracing-appender = "=0.2.5"
tracing-subscriber = { version = "=0.3.23", default-features = false, features = ["env-filter", "fmt", "std", "ansi"] }
unicode-segmentation = "=1.13.3"
unicode-width = "=0.2.2"
vt100 = "=0.18.1"
```

`crates/pawsman-core/Cargo.toml`:

```toml
[package]
name = "pawsman-core"
description = "pawsman engine: env snapshot, paths, settings, keys, notices"
version.workspace = true
edition.workspace = true
rust-version.workspace = true
license.workspace = true
publish.workspace = true

[dependencies]
pawsman-error.workspace = true
serde.workspace = true
toml.workspace = true
tracing.workspace = true

[dev-dependencies]
pawsman-test-support.workspace = true
rstest.workspace = true
tempfile.workspace = true

[lints]
workspace = true
```

`crates/pawsman-core/src/lib.rs`:

```rust
//! The pawsman engine. It has no terminal dependencies: the TUI and the CLI are thin over it.
#![forbid(unsafe_code)]

pub mod env;
```

- [ ] **Step 2: Write the failing tests** at the end of `env.rs`, with a stub whose methods return `false`/`None`:

```rust
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
    fn utf8_locale_takes_the_first_set_variable(#[case] pairs: &[(&str, &str)], #[case] expected: bool) {
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
        assert_eq!(env(&[("COLORTERM", value)]).truecolor(), expected, "COLORTERM={value}");
    }

    #[rstest]
    #[case::one("1", true)]
    #[case::word("yes", true)]
    #[case::zero("0", false)]
    #[case::empty("", false)]
    fn no_animation_unless_zero_or_empty(#[case] value: &str, #[case] expected: bool) {
        assert_eq!(env(&[("PAWSMAN_NO_ANIMATION", value)]).no_animation(), expected, "value {value:?}");
    }

    #[test]
    fn pawsman_log_wins_over_rust_log() {
        let both = env(&[("PAWSMAN_LOG", "pawsman=debug"), ("RUST_LOG", "trace")]);
        let rust_only = env(&[("RUST_LOG", "trace")]);

        assert_eq!(both.log_filter(), Some("pawsman=debug"), "PAWSMAN_LOG first");
        assert_eq!(rust_only.log_filter(), Some("trace"), "RUST_LOG as fallback");
    }

    #[test]
    fn tmux_needs_a_value() {
        assert!(env(&[("TMUX", "/tmp/tmux-1000/default,1,0")]).in_tmux(), "TMUX set");
        assert!(!env(&[]).in_tmux(), "TMUX unset");
    }
}
```

- [ ] **Step 3: Run and see them fail on assertions**

Run: `cargo nextest run -p pawsman-core`. Expected: the cases expecting `true` or `Some` fail with their messages.

- [ ] **Step 4: Commit the tests** — `git commit -m "test: env snapshot queries"`.

- [ ] **Step 5: Implement**

```rust
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
    #[expect(clippy::disallowed_methods, reason = "the one place that reads the process env (OBSERVABILITY P1)")]
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
        Self { vars: pairs.into_iter().map(|(k, v)| (k.into(), v.into())).collect() }
    }

    /// The value of `name`; an empty value counts as unset.
    #[must_use]
    pub fn var(&self, name: &str) -> Option<&str> {
        self.vars.get(name).map(String::as_str).filter(|v| !v.is_empty())
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
```

- [ ] **Step 6: Run the gates; commit** — `git commit -m "feat: env snapshot"`.

### Task 2: paths, settings, notices and core errors

**Files:**

- Create: `crates/pawsman-core/src/{paths,settings,notice,error,codes}.rs`, `crates/pawsman-test-support/{Cargo.toml,src/lib.rs,src/errors.rs}`, `docs/errors/core/no_home.md`, `docs/errors/core/settings_invalid.md`
- Modify: `crates/pawsman-core/src/lib.rs`

**Interfaces:**

- Consumes: `EnvSnapshot` (Task 1).
- Produces:
  - `Paths::from_env(&EnvSnapshot) -> Result<Paths, CoreError>`, `config_dir() -> &Path` (`…/pawsman`), `state_dir() -> &Path`, `settings_file() -> PathBuf`, `log_dir() -> &Path`.
  - `Settings::load(&Path) -> Result<Settings, CoreError>` (a missing file is `Ok(default)`), `Settings::ui() -> &Ui`, `Ui::animations() -> Animations`, `Ui::images() -> ImageMode`; `Animations::{On, Off}`; `ImageMode::{Auto, Kitty, Sixel, Blocks, Off}` with `FromStr` (error `UnknownImageMode`) and `Display`.
  - `Notice::{info, warn}(impl Into<String>)`, `Notice::from_error(&dyn ErrorInfo)`, `level() -> Level`, `text() -> &str`; `Level::{Info, Warn, Error}`.
  - `CoreError` (`ErrorInfo`), `codes::{NO_HOME, SETTINGS_INVALID, ALL}`.
  - `pawsman_test_support::errors::assert_pages(namespace: &str, codes: &[ErrorCode])`.

- [ ] **Step 1: Test-support crate**

`crates/pawsman-test-support/Cargo.toml`:

```toml
[package]
name = "pawsman-test-support"
description = "Fakes and fixtures for pawsman tests"
version.workspace = true
edition.workspace = true
rust-version.workspace = true
license.workspace = true
publish = false

[dependencies]
pawsman-error.workspace = true

[lints]
workspace = true
```

`src/lib.rs`:

```rust
//! Fakes and fixtures shared by pawsman tests.
#![forbid(unsafe_code)]

pub mod errors;
```

`src/errors.rs`:

```rust
//! The error-code registry check: every code is unique, in its crate's namespace, and has a page.

use std::collections::BTreeSet;
use std::path::PathBuf;

use pawsman_error::ErrorCode;

/// Panics naming every code in `codes` that is duplicated, outside `namespace`, or without a page
/// at `docs/errors/<namespace>/<slug>.md`.
#[expect(clippy::panic, reason = "a test assertion helper")]
pub fn assert_pages(namespace: &str, codes: &[ErrorCode]) {
    let docs = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../docs/errors");
    let mut seen = BTreeSet::new();
    let mut problems = Vec::new();
    for code in codes {
        if !seen.insert(code.as_str()) {
            problems.push(format!("{code}: listed twice"));
        }
        if code.namespace() != namespace {
            problems.push(format!("{code}: not in `{namespace}`"));
        }
        let page = docs.join(code.namespace()).join(format!("{}.md", code.slug()));
        if !page.is_file() {
            problems.push(format!("{code}: no page at {}", page.display()));
        }
    }
    if !problems.is_empty() {
        panic!("error registry: {}", problems.join("; "));
    }
}
```

- [ ] **Step 2: Write the failing tests** (stubs: `Paths::from_env` returns `Err(CoreError::no_home())`, `Settings::load` returns `Ok(Settings::default())`, `codes::ALL` is empty).

`paths.rs` tests:

```rust
#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    #[test]
    fn xdg_directories_win_over_home() {
        let env = EnvSnapshot::from_pairs([("HOME", "/home/u"), ("XDG_CONFIG_HOME", "/cfg"), ("XDG_STATE_HOME", "/st")]);

        let paths = Paths::from_env(&env).unwrap();

        assert_eq!(paths.config_dir(), Path::new("/cfg/pawsman"), "XDG_CONFIG_HOME");
        assert_eq!(paths.state_dir(), Path::new("/st/pawsman"), "XDG_STATE_HOME");
        assert_eq!(paths.settings_file(), Path::new("/cfg/pawsman/settings.toml"), "settings file");
    }

    #[test]
    fn home_is_the_fallback_and_relative_xdg_is_ignored() {
        let env = EnvSnapshot::from_pairs([("HOME", "/home/u"), ("XDG_CONFIG_HOME", "relative")]);

        let paths = Paths::from_env(&env).unwrap();

        assert_eq!(paths.config_dir(), Path::new("/home/u/.config/pawsman"), "relative XDG ignored");
        assert_eq!(paths.log_dir(), Path::new("/home/u/.local/state/pawsman"), "state under HOME");
    }

    #[test]
    fn nothing_set_is_no_home() {
        let err = Paths::from_env(&EnvSnapshot::default()).unwrap_err();

        assert_eq!(err.code(), crate::codes::NO_HOME, "code");
    }
}
```

`settings.rs` tests:

```rust
#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::*;

    fn write(text: &str) -> tempfile::NamedTempFile {
        let mut file = tempfile::NamedTempFile::new().unwrap();
        file.write_all(text.as_bytes()).unwrap();
        file
    }

    #[test]
    fn missing_file_gives_defaults() {
        let dir = tempfile::tempdir().unwrap();

        let settings = Settings::load(&dir.path().join("settings.toml")).unwrap();

        assert_eq!(settings, Settings::default(), "defaults");
        assert_eq!(settings.ui().animations(), Animations::On, "animations on by default");
        assert_eq!(settings.ui().images(), ImageMode::Auto, "images auto by default");
    }

    #[test]
    fn ui_values_are_read() {
        let file = write("[ui]\nanimations = false\nimages = \"blocks\"\n");

        let settings = Settings::load(file.path()).unwrap();

        assert_eq!(settings.ui().animations(), Animations::Off, "animations off");
        assert_eq!(settings.ui().images(), ImageMode::Blocks, "images blocks");
    }

    #[test]
    fn unknown_key_is_settings_invalid_naming_the_file() {
        let file = write("[ui]\nanimation = false\n");

        let err = Settings::load(file.path()).unwrap_err();

        assert_eq!(err.code(), crate::codes::SETTINGS_INVALID, "code");
        assert!(err.to_string().contains("animation"), "names the key: {err}");
        assert!(!err.to_string().contains('\n'), "one line: {err}");
    }

    #[test]
    fn image_mode_parses_and_rejects() {
        assert_eq!("sixel".parse::<ImageMode>().unwrap(), ImageMode::Sixel, "sixel");
        let err = "png".parse::<ImageMode>().unwrap_err();
        assert!(err.to_string().contains("`png`"), "names the value: {err}");
    }
}
```

`codes.rs` tests:

```rust
#[cfg(test)]
mod tests {
    #[test]
    fn every_code_has_a_page() {
        pawsman_test_support::errors::assert_pages("core", super::ALL);
    }
}
```

`notice.rs` tests:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notice_from_error_carries_the_code() {
        let err = crate::CoreError::no_home();

        let notice = Notice::from_error(&err);

        assert_eq!(notice.level(), Level::Error, "errors are Error notices");
        assert!(notice.text().contains("core::no_home"), "code in the text: {}", notice.text());
    }
}
```

- [ ] **Step 3: Run; expect assertion failures** (registry finds no codes yet: make the stub `ALL = &[NO_HOME]` without pages so `assert_pages` panics with "no page"). Commit `test: paths, settings, notices, core error registry`.

- [ ] **Step 4: Implement**

`error.rs`:

```rust
//! The core crate's error.

use std::borrow::Cow;
use std::fmt;
use std::path::{Path, PathBuf};

use pawsman_error::{ErrorCode, ErrorInfo};

use crate::codes;

/// An error from `pawsman-core`.
#[derive(Debug)]
#[non_exhaustive]
pub struct CoreError {
    kind: Kind,
}

#[derive(Debug)]
enum Kind {
    NoHome,
    SettingsInvalid { path: PathBuf, reason: String },
}

impl CoreError {
    pub(crate) fn no_home() -> Self {
        Self { kind: Kind::NoHome }
    }

    pub(crate) fn settings_invalid(path: &Path, reason: impl Into<String>) -> Self {
        Self { kind: Kind::SettingsInvalid { path: path.to_owned(), reason: reason.into() } }
    }
}

impl fmt::Display for CoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.kind {
            Kind::NoHome => f.write_str("neither `HOME` nor the `XDG_*` directories are set"),
            Kind::SettingsInvalid { path, reason } => write!(f, "`{}` is not valid: {reason}", path.display()),
        }
    }
}

impl std::error::Error for CoreError {}

impl ErrorInfo for CoreError {
    fn code(&self) -> ErrorCode {
        match self.kind {
            Kind::NoHome => codes::NO_HOME,
            Kind::SettingsInvalid { .. } => codes::SETTINGS_INVALID,
        }
    }

    fn help(&self) -> Option<Cow<'_, str>> {
        Some(Cow::Borrowed(match self.kind {
            Kind::NoHome => "Set `HOME` to your home directory.",
            Kind::SettingsInvalid { .. } => "Fix the file, or remove it to use the defaults.",
        }))
    }
}
```

`codes.rs`:

```rust
//! Error codes of `pawsman-core`.

use pawsman_error::ErrorCode;

/// Neither `HOME` nor `XDG_*` is set.
pub const NO_HOME: ErrorCode = ErrorCode::new("core::no_home");
/// `settings.toml` cannot be read or parsed.
pub const SETTINGS_INVALID: ErrorCode = ErrorCode::new("core::settings_invalid");

/// Every code, for the registry test.
pub const ALL: &[ErrorCode] = &[NO_HOME, SETTINGS_INVALID];
```

`paths.rs`:

```rust
//! Where pawsman keeps configuration and state: `XDG_*` when absolute, else under `$HOME`, on Linux
//! and macOS alike.

use std::path::{Path, PathBuf};

use crate::CoreError;
use crate::env::EnvSnapshot;

/// pawsman's directories.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Paths {
    config: PathBuf,
    state: PathBuf,
}

impl Paths {
    /// Resolves the directories from the env snapshot.
    ///
    /// # Errors
    ///
    /// `core::no_home` when a directory has neither an absolute `XDG_*` value nor `HOME`.
    pub fn from_env(env: &EnvSnapshot) -> Result<Self, CoreError> {
        let dir = |xdg: &str, fallback: &str| -> Result<PathBuf, CoreError> {
            let base = env
                .var(xdg)
                .map(PathBuf::from)
                .filter(|p| p.is_absolute())
                .or_else(|| env.var("HOME").map(|home| Path::new(home).join(fallback)))
                .ok_or_else(CoreError::no_home)?;
            Ok(base.join("pawsman"))
        };
        Ok(Self { config: dir("XDG_CONFIG_HOME", ".config")?, state: dir("XDG_STATE_HOME", ".local/state")? })
    }

    /// `…/pawsman` under the config directory.
    #[must_use]
    pub fn config_dir(&self) -> &Path {
        &self.config
    }

    /// `…/pawsman` under the state directory.
    #[must_use]
    pub fn state_dir(&self) -> &Path {
        &self.state
    }

    /// `settings.toml` in the config directory.
    #[must_use]
    pub fn settings_file(&self) -> PathBuf {
        self.config.join("settings.toml")
    }

    /// Where the rolling log goes.
    #[must_use]
    pub fn log_dir(&self) -> &Path {
        &self.state
    }
}
```

`settings.rs`:

```rust
//! User settings from `settings.toml`. Unknown keys are an error; the caller shows it and uses the
//! defaults.

use std::fmt;
use std::path::Path;
use std::str::FromStr;

use serde::Deserialize;

use crate::CoreError;

/// Everything in `settings.toml`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct Settings {
    ui: Ui,
}

/// The `[ui]` table.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct Ui {
    animations: Animations,
    images: ImageMode,
}

/// Whether the startup animation and other motion play.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(from = "bool")]
pub enum Animations {
    /// Play them.
    #[default]
    On,
    /// Show final frames only.
    Off,
}

impl From<bool> for Animations {
    fn from(on: bool) -> Self {
        if on { Self::On } else { Self::Off }
    }
}

/// How images are drawn; `Auto` follows the probe.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ImageMode {
    /// Pick from the probed capabilities.
    #[default]
    Auto,
    /// Kitty graphics protocol through Unicode placeholders.
    Kitty,
    /// Sixel.
    Sixel,
    /// Half blocks.
    Blocks,
    /// A text placeholder.
    Off,
}

impl ImageMode {
    const NAMES: [(&'static str, Self); 5] =
        [("auto", Self::Auto), ("kitty", Self::Kitty), ("sixel", Self::Sixel), ("blocks", Self::Blocks), ("off", Self::Off)];
}

impl fmt::Display for ImageMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = Self::NAMES.iter().find(|(_, mode)| mode == self).map_or("auto", |(name, _)| name);
        f.write_str(name)
    }
}

/// A value that is not an image mode.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnknownImageMode(String);

impl fmt::Display for UnknownImageMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "unknown image mode `{}`, expected auto, kitty, sixel, blocks or off", self.0)
    }
}

impl std::error::Error for UnknownImageMode {}

impl FromStr for ImageMode {
    type Err = UnknownImageMode;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::NAMES.iter().find(|(name, _)| *name == s).map(|(_, mode)| *mode).ok_or_else(|| UnknownImageMode(s.to_owned()))
    }
}

impl Settings {
    /// Reads `path`; a missing file gives the defaults.
    ///
    /// # Errors
    ///
    /// `core::settings_invalid` when the file cannot be read or parsed.
    pub fn load(path: &Path) -> Result<Self, CoreError> {
        let text = match std::fs::read_to_string(path) {
            Ok(text) => text,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(err) => return Err(CoreError::settings_invalid(path, err.to_string())),
        };
        toml::from_str(&text).map_err(|err| CoreError::settings_invalid(path, err.message().replace('\n', " ")))
    }

    /// The `[ui]` table.
    #[must_use]
    pub fn ui(&self) -> &Ui {
        &self.ui
    }
}

impl Ui {
    /// `animations`.
    #[must_use]
    pub fn animations(&self) -> Animations {
        self.animations
    }

    /// `images`.
    #[must_use]
    pub fn images(&self) -> ImageMode {
        self.images
    }
}
```

`notice.rs`:

```rust
//! Short messages shown in the status line.

use pawsman_error::ErrorInfo;

/// How a notice is styled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Level {
    /// Neutral information.
    Info,
    /// Something degraded.
    Warn,
    /// Something failed.
    Error,
}

/// A one-line message for the status line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notice {
    level: Level,
    text: String,
}

impl Notice {
    /// An information notice.
    pub fn info(text: impl Into<String>) -> Self {
        Self { level: Level::Info, text: text.into() }
    }

    /// A warning notice.
    pub fn warn(text: impl Into<String>) -> Self {
        Self { level: Level::Warn, text: text.into() }
    }

    /// An error notice: the code, then the message, so a truncated line still shows the code.
    pub fn from_error(err: &dyn ErrorInfo) -> Self {
        Self { level: Level::Error, text: format!("{}: {err}", err.code()) }
    }

    /// The level.
    #[must_use]
    pub fn level(&self) -> Level {
        self.level
    }

    /// The text.
    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }
}
```

`lib.rs` adds `pub mod codes; mod error; pub mod notice; pub mod paths; pub mod settings; pub use error::CoreError;`.

Pages, following [docs/errors/README.md](../../errors/README.md) (`core/no_home.md`; `settings_invalid.md` in the same shape):

```markdown
# `core::no_home`

pawsman cannot find a home directory for its settings and log.

## When it happens

`HOME` is unset or empty and no absolute `XDG_CONFIG_HOME` or `XDG_STATE_HOME` is set, as in some service managers and stripped `env -i` shells. pawsman still runs, without a log and with default settings.

## How to fix it

Set `HOME`, or set `XDG_CONFIG_HOME` and `XDG_STATE_HOME` to absolute paths.

## Related

- [`core::settings_invalid`](settings_invalid.md)
```

- [ ] **Step 5: Gates; commit** `feat: paths, settings, notices and core errors`.

### Task 3: key types, the key table and layout folding

**Files:**

- Create: `crates/pawsman-core/src/keys.rs`, `keys/chord.rs`, `keys/fold.rs`

**Interfaces:**

- Produces:
  - `keys::Mods` (`NONE`, `SHIFT`, `ALT`, `CTRL`, `SUPER`, `HYPER`, `META`; `contains`, `is_empty`, `from_bits(u8)`, `BitOr`).
  - `keys::KeyCode::{Char(char), Enter, Esc, Tab, BackTab, Backspace, Up, Down, Left, Right, Home, End, PageUp, PageDown, Insert, Delete, F(u8)}`.
  - `keys::Chord::{new(KeyCode, Mods), plain(KeyCode), ctrl(char)}`, `code()`, `mods()`, `Display` (`Ctrl-Q`, `F1`, `Esc`, `Alt-Enter`).
  - `keys::Action::{Quit, Help, Close, Copy}`, `keys::Context::{Splash, Home, Help}`, `keys::Binding` (`action()`, `chords()`, `hint()`, `in_bar() -> bool`), `keys::table() -> &'static [Binding]`, `keys::lookup(Context, Chord) -> Option<Action>`, `keys::hints(Context) -> Vec<(String, &'static str)>`.
  - `keys::fold(char) -> char`: a Cyrillic ЙЦУКЕН letter to the US key in the same place, anything else unchanged.

- [ ] **Step 1: Failing tests** in `keys.rs` (stubs: `lookup` returns `None`, `fold` returns its input, `Display` writes nothing):

```rust
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
    fn lookup_finds_the_bound_action(#[case] context: Context, #[case] chord: Chord, #[case] expected: Option<Action>) {
        assert_eq!(lookup(context, chord), expected, "{chord} in {context:?}");
    }

    #[test]
    fn cyrillic_ctrl_q_quits() {
        let chord = Chord::new(KeyCode::Char('й'), Mods::CTRL);

        assert_eq!(lookup(Context::Home, chord), Some(Action::Quit), "Ctrl-й is Ctrl-Q");
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
    fn chords_display_like_the_docs(#[case] chord: Chord, #[case] expected: &str) {
        assert_eq!(chord.to_string(), expected, "label");
    }

    #[test]
    fn home_hints_name_quit_and_help() {
        let hints = hints(Context::Home);

        assert_eq!(hints, vec![("Ctrl-Q".to_owned(), "quit"), ("F1".to_owned(), "help")], "bar hints on home");
    }

    #[test]
    fn every_context_can_quit() {
        for context in [Context::Splash, Context::Home, Context::Help] {
            assert_eq!(lookup(context, Chord::ctrl('q')), Some(Action::Quit), "{context:?}");
        }
    }
}
```

- [ ] **Step 2: Run; see failures; commit** `test: key table, chords and layout folding`.

- [ ] **Step 3: Implement**

`keys/chord.rs`:

```rust
//! A key with its modifiers, independent of any terminal library.

use std::fmt;
use std::ops::BitOr;

/// Modifier bits, as in the kitty keyboard protocol (value minus one).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Mods(u8);

impl Mods {
    /// No modifier.
    pub const NONE: Self = Self(0);
    /// Shift.
    pub const SHIFT: Self = Self(1);
    /// Alt (Option).
    pub const ALT: Self = Self(2);
    /// Control.
    pub const CTRL: Self = Self(4);
    /// Super (Cmd, Windows).
    pub const SUPER: Self = Self(8);
    /// Hyper.
    pub const HYPER: Self = Self(16);
    /// Meta.
    pub const META: Self = Self(32);

    /// Keeps the six modifier bits; Caps Lock and Num Lock are dropped.
    #[must_use]
    pub const fn from_bits(bits: u8) -> Self {
        Self(bits & 0b11_1111)
    }

    /// All of `other` is set.
    #[must_use]
    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    /// No modifier is set.
    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }
}

impl BitOr for Mods {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self {
        Self(self.0 | rhs.0)
    }
}

/// A key, without modifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum KeyCode {
    /// A character key; letters are lowercase when Ctrl or Alt is held.
    Char(char),
    /// Enter.
    Enter,
    /// Escape.
    Esc,
    /// Tab.
    Tab,
    /// Shift-Tab.
    BackTab,
    /// Backspace.
    Backspace,
    /// Up arrow.
    Up,
    /// Down arrow.
    Down,
    /// Left arrow.
    Left,
    /// Right arrow.
    Right,
    /// Home.
    Home,
    /// End.
    End,
    /// Page Up.
    PageUp,
    /// Page Down.
    PageDown,
    /// Insert.
    Insert,
    /// Delete.
    Delete,
    /// A function key, F1 to F24.
    F(u8),
}

/// A key and its modifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Chord {
    code: KeyCode,
    mods: Mods,
}

impl Chord {
    /// A key with modifiers.
    #[must_use]
    pub const fn new(code: KeyCode, mods: Mods) -> Self {
        Self { code, mods }
    }

    /// A key without modifiers.
    #[must_use]
    pub const fn plain(code: KeyCode) -> Self {
        Self { code, mods: Mods::NONE }
    }

    /// Ctrl plus a lowercase character.
    #[must_use]
    pub const fn ctrl(c: char) -> Self {
        Self { code: KeyCode::Char(c), mods: Mods::CTRL }
    }

    /// The key.
    #[must_use]
    pub const fn code(self) -> KeyCode {
        self.code
    }

    /// The modifiers.
    #[must_use]
    pub const fn mods(self) -> Mods {
        self.mods
    }
}

impl fmt::Display for Chord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (bit, name) in [(Mods::CTRL, "Ctrl-"), (Mods::ALT, "Alt-"), (Mods::SHIFT, "Shift-"), (Mods::SUPER, "Super-")] {
            if self.mods.contains(bit) {
                f.write_str(name)?;
            }
        }
        match self.code {
            KeyCode::Char(' ') => f.write_str("Space"),
            KeyCode::Char(c) => write!(f, "{}", c.to_uppercase()),
            KeyCode::F(n) => write!(f, "F{n}"),
            other => {
                let name = match other {
                    KeyCode::Enter => "Enter",
                    KeyCode::Esc => "Esc",
                    KeyCode::Tab => "Tab",
                    KeyCode::BackTab => "Shift-Tab",
                    KeyCode::Backspace => "Backspace",
                    KeyCode::Up => "Up",
                    KeyCode::Down => "Down",
                    KeyCode::Left => "Left",
                    KeyCode::Right => "Right",
                    KeyCode::Home => "Home",
                    KeyCode::End => "End",
                    KeyCode::PageUp => "PgUp",
                    KeyCode::PageDown => "PgDn",
                    KeyCode::Insert => "Ins",
                    KeyCode::Delete => "Del",
                    KeyCode::Char(_) | KeyCode::F(_) => "",
                };
                f.write_str(name)
            }
        }
    }
}
```

`keys/fold.rs`:

```rust
//! Layout folding: a key on a Cyrillic ЙЦУКЕН layout acts as the US key in the same place, so
//! commands work without switching layouts. Applied to commands only, never to typed text.

const CYRILLIC: &str = "йцукенгшщзхъфывапролджэячсмитьбю";
const US: &str = "qwertyuiop[]asdfghjkl;'zxcvbnm,.";

/// The US key in the same place as `c`, or `c` itself.
#[must_use]
pub fn fold(c: char) -> char {
    let lower = c.to_lowercase().next().unwrap_or(c);
    if lower == 'ё' {
        return '`';
    }
    CYRILLIC.chars().zip(US.chars()).find(|(cyr, _)| *cyr == lower).map_or(c, |(_, us)| us)
}
```

`keys.rs`:

```rust
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
}

/// Where a binding applies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Context {
    /// The startup splash.
    Splash,
    /// The main view.
    Home,
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

const ALL: &[Context] = &[Context::Splash, Context::Home, Context::Help];

static TABLE: &[Binding] = &[
    Binding { action: Action::Quit, chords: &[Chord::ctrl('q')], contexts: ALL, hint: "quit", shown: Shown::Bar },
    Binding {
        action: Action::Help,
        chords: &[Chord::plain(KeyCode::F(1))],
        contexts: &[Context::Home],
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
        contexts: &[Context::Home],
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
        .find(|b| b.contexts.contains(&context) && b.chords.iter().any(|c| *c == chord || *c == folded))
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
```

`lib.rs` adds `pub mod keys;`.

- [ ] **Step 4: Gates; commit** `feat: key table with layout folding`.

### Task 4: the input decoder

**Files:**

- Create: `crates/pawsman-tui/Cargo.toml`, `src/lib.rs`, `src/input.rs`, `src/input/decoder.rs`

**Interfaces:**

- Consumes: `pawsman_core::keys::{Chord, KeyCode, Mods}`.
- Produces: `input::Input::{Key(Chord), Paste(String), Mouse(Mouse), FocusGained, FocusLost}`; `Mouse { kind: MouseKind, column: u16, row: u16, mods: Mods }` with getters; `MouseKind::{Press(Button), Release(Button), Drag(Button), ScrollUp, ScrollDown}`; `Button::{Left, Middle, Right}`; `Decoder::new()`, `feed(&mut self, &[u8]) -> Vec<Input>`, `flush(&mut self) -> Vec<Input>` (called after `ESC_TIMEOUT` with no bytes), `has_pending(&self) -> bool`; `input::ESC_TIMEOUT: Duration` (25 ms); `input::MAX_SEQUENCE` (64 KiB), `input::MAX_PASTE` (8 MiB).

- [ ] **Step 1: Crate**

```toml
[package]
name = "pawsman-tui"
description = "pawsman terminal UI: terminal ownership, probe, input, views"
version.workspace = true
edition.workspace = true
rust-version.workspace = true
license.workspace = true
publish.workspace = true

[dependencies]
pawsman-core.workspace = true
pawsman-error.workspace = true
crossterm.workspace = true
ratatui.workspace = true
rustix.workspace = true
signal-hook.workspace = true
tachyonfx.workspace = true
tracing.workspace = true
unicode-segmentation.workspace = true
unicode-width.workspace = true

[dev-dependencies]
fontdue.workspace = true
image.workspace = true
insta.workspace = true
pawsman-test-support.workspace = true
rstest.workspace = true

[lints]
workspace = true
```

`lib.rs` (grows in later tasks):

```rust
//! The pawsman terminal UI. It owns the terminal; everything else lives in `pawsman-core`.
#![forbid(unsafe_code)]

pub mod input;
```

`input.rs`:

```rust
//! Keyboard, mouse, paste and focus input, decoded by pawsman itself from `/dev/tty`.

mod decoder;

use std::time::Duration;

pub use decoder::{Button, Decoder, Input, Mouse, MouseKind};

/// How long a lone `ESC` waits for the rest of a sequence before it counts as the Esc key.
pub const ESC_TIMEOUT: Duration = Duration::from_millis(25);
/// The longest escape sequence or string kept; a longer one is dropped.
pub const MAX_SEQUENCE: usize = 64 * 1024;
/// The largest paste kept; the rest of a longer paste is dropped.
pub const MAX_PASTE: usize = 8 * 1024 * 1024;
```

- [ ] **Step 2: Failing tests** at the end of `decoder.rs` (stub `feed`/`flush` return empty `Vec`s):

```rust
#[cfg(test)]
mod tests {
    use pawsman_core::keys::{Chord, KeyCode, Mods};
    use rstest::rstest;

    use super::*;

    fn keys(bytes: &[u8]) -> Vec<Input> {
        let mut d = Decoder::new();
        let mut out = d.feed(bytes);
        out.extend(d.flush());
        out
    }

    fn key(code: KeyCode, mods: Mods) -> Input {
        Input::Key(Chord::new(code, mods))
    }

    #[rstest]
    #[case::letter(b"a", key(KeyCode::Char('a'), Mods::NONE))]
    #[case::shifted_text(b"A", key(KeyCode::Char('A'), Mods::NONE))]
    #[case::cyrillic(b"\xd0\xb6", key(KeyCode::Char('ж'), Mods::NONE))]
    #[case::enter(b"\r", key(KeyCode::Enter, Mods::NONE))]
    #[case::tab(b"\t", key(KeyCode::Tab, Mods::NONE))]
    #[case::backspace(b"\x7f", key(KeyCode::Backspace, Mods::NONE))]
    #[case::ctrl_q_legacy(b"\x11", key(KeyCode::Char('q'), Mods::CTRL))]
    #[case::ctrl_c_legacy(b"\x03", key(KeyCode::Char('c'), Mods::CTRL))]
    #[case::lone_esc(b"\x1b", key(KeyCode::Esc, Mods::NONE))]
    #[case::alt_x(b"\x1bx", key(KeyCode::Char('x'), Mods::ALT))]
    #[case::alt_enter(b"\x1b\r", key(KeyCode::Enter, Mods::ALT))]
    #[case::up(b"\x1b[A", key(KeyCode::Up, Mods::NONE))]
    #[case::ss3_up(b"\x1bOA", key(KeyCode::Up, Mods::NONE))]
    #[case::ctrl_right(b"\x1b[1;5C", key(KeyCode::Right, Mods::CTRL))]
    #[case::f1_ss3(b"\x1bOP", key(KeyCode::F(1), Mods::NONE))]
    #[case::f5(b"\x1b[15~", key(KeyCode::F(5), Mods::NONE))]
    #[case::shift_f5(b"\x1b[15;2~", key(KeyCode::F(5), Mods::SHIFT))]
    #[case::delete(b"\x1b[3~", key(KeyCode::Delete, Mods::NONE))]
    #[case::backtab(b"\x1b[Z", key(KeyCode::BackTab, Mods::NONE))]
    #[case::kitty_ctrl_q(b"\x1b[113;5u", key(KeyCode::Char('q'), Mods::CTRL))]
    #[case::kitty_ctrl_enter(b"\x1b[13;5u", key(KeyCode::Enter, Mods::CTRL))]
    #[case::kitty_esc(b"\x1b[27u", key(KeyCode::Esc, Mods::NONE))]
    #[case::kitty_cyrillic_ctrl(b"\x1b[1081;5u", key(KeyCode::Char('й'), Mods::CTRL))]
    #[case::kitty_caps_lock_ignored(b"\x1b[113;69u", key(KeyCode::Char('q'), Mods::CTRL))]
    #[case::kitty_f1(b"\x1b[P", key(KeyCode::F(1), Mods::NONE))]
    fn one_key(#[case] bytes: &[u8], #[case] expected: Input) {
        assert_eq!(keys(bytes), vec![expected], "bytes {bytes:?}");
    }

    #[test]
    fn kitty_release_events_are_dropped() {
        assert_eq!(keys(b"\x1b[113;5:3u"), vec![], "release");
    }

    #[rstest]
    #[case::graphics(b"\x1b_Gi=31;OK\x1b\\")]
    #[case::keyboard_flags(b"\x1b[?1u")]
    #[case::da1(b"\x1b[?62;22c")]
    #[case::cell_size(b"\x1b[6;20;10t")]
    #[case::decrqss(b"\x1bP1$r48:2:1:2:3m\x1b\\")]
    #[case::osc(b"\x1b]52;c;aGk=\x07")]
    fn late_probe_replies_type_nothing(#[case] bytes: &[u8]) {
        let mut with_key = bytes.to_vec();
        with_key.extend_from_slice(b"z");

        assert_eq!(keys(&with_key), vec![key(KeyCode::Char('z'), Mods::NONE)], "only the key after {bytes:?}");
    }

    #[test]
    fn paste_arrives_whole_with_control_bytes_inside() {
        let out = keys(b"\x1b[200~line one\r\n\x1b[Ax\x1b[201~q");

        assert_eq!(
            out,
            vec![Input::Paste("line one\r\n\x1b[Ax".to_owned()), key(KeyCode::Char('q'), Mods::NONE)],
            "paste then key"
        );
    }

    #[test]
    fn sequences_split_across_reads_decode_once() {
        let mut d = Decoder::new();

        let first = d.feed(b"\x1b[11");
        let second = d.feed(b"3;5u");

        assert_eq!(first, vec![], "nothing yet");
        assert_eq!(second, vec![key(KeyCode::Char('q'), Mods::CTRL)], "whole chord");
    }

    #[test]
    fn split_utf8_waits_for_the_rest() {
        let mut d = Decoder::new();

        let first = d.feed(b"\xd0");
        let second = d.feed(b"\xb6");

        assert_eq!(first, vec![], "half a character");
        assert_eq!(second, vec![key(KeyCode::Char('ж'), Mods::NONE)], "whole character");
    }

    #[test]
    fn focus_and_sgr_mouse() {
        let out = keys(b"\x1b[I\x1b[<0;10;5M\x1b[<64;1;1M\x1b[<32;11;5M\x1b[<0;11;5m\x1b[O");

        assert_eq!(
            out,
            vec![
                Input::FocusGained,
                Input::Mouse(Mouse::new(MouseKind::Press(Button::Left), 9, 4, Mods::NONE)),
                Input::Mouse(Mouse::new(MouseKind::ScrollUp, 0, 0, Mods::NONE)),
                Input::Mouse(Mouse::new(MouseKind::Drag(Button::Left), 10, 4, Mods::NONE)),
                Input::Mouse(Mouse::new(MouseKind::Release(Button::Left), 10, 4, Mods::NONE)),
                Input::FocusLost,
            ],
            "focus, press, wheel, drag, release, blur"
        );
    }

    #[test]
    fn unterminated_string_is_dropped_at_the_cap() {
        let mut d = Decoder::new();
        let mut junk = b"\x1bP".to_vec();
        junk.resize(crate::input::MAX_SEQUENCE + 10, b'x');

        let during = d.feed(&junk);
        let after = d.feed(b"\x1b[A");

        assert_eq!(during, vec![], "the string types nothing");
        assert_eq!(after, vec![key(KeyCode::Up, Mods::NONE)], "keys work again after the cap");
    }

    #[test]
    fn paste_is_capped() {
        let mut d = Decoder::new();
        let mut paste = b"\x1b[200~".to_vec();
        paste.resize(crate::input::MAX_PASTE + 100, b'a');
        paste.extend_from_slice(b"\x1b[201~");

        let out = d.feed(&paste);

        let [Input::Paste(text)] = out.as_slice() else { panic!("one paste, got {} inputs", out.len()) };
        assert_eq!(text.len(), crate::input::MAX_PASTE, "capped");
    }

    #[test]
    fn pending_only_while_a_sequence_is_open() {
        let mut d = Decoder::new();

        d.feed(b"\x1b");
        let open = d.has_pending();
        d.flush();

        assert!(open, "a lone ESC is pending");
        assert!(!d.has_pending(), "flushed");
    }
}
```

- [ ] **Step 3: Run; see failures; commit** `test: input decoder`.

- [ ] **Step 4: Implement** `decoder.rs`. The decoder keeps unconsumed bytes in `pending` and an optional paste buffer; `feed` appends and parses as far as complete sequences go; `flush` turns a lone `ESC` into Esc and drops any other incomplete sequence.

```rust
//! A pure, stateful decoder from terminal bytes to inputs: legacy keys, kitty `CSI … u` under the
//! disambiguate flag, SGR mouse, bracketed paste and focus. Terminal replies (APC, DCS, OSC, and the
//! probe's CSI replies) are dropped, so a late probe reply never types anything.

use pawsman_core::keys::{Chord, KeyCode, Mods};
use tracing::debug;

use super::{MAX_PASTE, MAX_SEQUENCE};

const ESC: u8 = 0x1b;
const PASTE_END: &[u8] = b"\x1b[201~";

/// One decoded input.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Input {
    /// A key press or repeat.
    Key(Chord),
    /// Text from bracketed paste.
    Paste(String),
    /// A mouse event.
    Mouse(Mouse),
    /// The terminal window gained focus.
    FocusGained,
    /// The terminal window lost focus.
    FocusLost,
}

/// A mouse button.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Button {
    /// Left.
    Left,
    /// Middle.
    Middle,
    /// Right.
    Right,
}

/// What the mouse did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseKind {
    /// Button pressed.
    Press(Button),
    /// Button released.
    Release(Button),
    /// Moved with the button held.
    Drag(Button),
    /// Wheel up.
    ScrollUp,
    /// Wheel down.
    ScrollDown,
}

/// A mouse event at a zero-based cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Mouse {
    kind: MouseKind,
    column: u16,
    row: u16,
    mods: Mods,
}

impl Mouse {
    /// A mouse event.
    #[must_use]
    pub const fn new(kind: MouseKind, column: u16, row: u16, mods: Mods) -> Self {
        Self { kind, column, row, mods }
    }

    /// What happened.
    #[must_use]
    pub const fn kind(self) -> MouseKind {
        self.kind
    }

    /// Zero-based column.
    #[must_use]
    pub const fn column(self) -> u16 {
        self.column
    }

    /// Zero-based row.
    #[must_use]
    pub const fn row(self) -> u16 {
        self.row
    }

    /// Held modifiers.
    #[must_use]
    pub const fn mods(self) -> Mods {
        self.mods
    }
}

/// Decodes bytes into inputs across reads.
#[derive(Debug, Default)]
pub struct Decoder {
    pending: Vec<u8>,
    paste: Option<Vec<u8>>,
}

/// What one parse step found at the front of `pending`.
enum Step {
    /// Consumed `usize` bytes, maybe producing an input.
    Done(usize, Option<Input>),
    /// Needs more bytes.
    Incomplete,
}

impl Decoder {
    /// A decoder with nothing pending.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether bytes are waiting for the rest of a sequence.
    #[must_use]
    pub fn has_pending(&self) -> bool {
        !self.pending.is_empty() || self.paste.is_some()
    }

    /// Decodes `bytes`, keeping an incomplete tail for the next call.
    pub fn feed(&mut self, bytes: &[u8]) -> Vec<Input> {
        self.pending.extend_from_slice(bytes);
        let mut out = Vec::new();
        loop {
            if self.paste.is_some() {
                if let Some(text) = self.take_paste() {
                    out.push(Input::Paste(text));
                    continue;
                }
                break;
            }
            match self.step() {
                Step::Done(n, input) => {
                    self.pending.drain(..n.min(self.pending.len()));
                    out.extend(input);
                }
                Step::Incomplete => {
                    if self.pending.len() > MAX_SEQUENCE {
                        debug!(bytes = self.pending.len(), "dropped an over-long sequence");
                        self.pending.clear();
                    }
                    break;
                }
            }
            if self.pending.is_empty() {
                break;
            }
        }
        out
    }

    /// Ends whatever is pending after `ESC_TIMEOUT` of silence: a lone `ESC` is the Esc key, any
    /// other incomplete sequence is dropped.
    pub fn flush(&mut self) -> Vec<Input> {
        let lone_esc = self.pending == [ESC];
        self.pending.clear();
        if lone_esc { vec![Input::Key(Chord::plain(KeyCode::Esc))] } else { Vec::new() }
    }

    /// Moves paste bytes out of `pending`, up to `MAX_PASTE`; the text once the end marker arrived.
    fn take_paste(&mut self) -> Option<String> {
        let end = find(&self.pending, PASTE_END);
        // Without the end marker, keep a tail that may be its start.
        let take = end.unwrap_or_else(|| self.pending.len().saturating_sub(PASTE_END.len() - 1));
        let paste = self.paste.get_or_insert_with(Vec::new);
        let room = MAX_PASTE.saturating_sub(paste.len());
        paste.extend(self.pending.iter().take(take.min(room)));
        self.pending.drain(..take);
        end?;
        self.pending.drain(..PASTE_END.len());
        self.paste.take().map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
    }

    fn step(&mut self) -> Step {
        let Some(&first) = self.pending.first() else { return Step::Incomplete };
        if first != ESC {
            return plain(&self.pending, Mods::NONE, 0);
        }
        match self.pending.get(1) {
            None => Step::Incomplete,
            Some(b'[') => self.csi(),
            Some(b'O') => ss3(&self.pending),
            Some(b'_' | b'P' | b']' | b'X' | b'^') => string(&self.pending),
            Some(&ESC) => Step::Done(1, Some(Input::Key(Chord::new(KeyCode::Esc, Mods::ALT)))),
            Some(_) => plain(&self.pending, Mods::ALT, 1),
        }
    }

    fn csi(&mut self) -> Step {
        let body = self.pending.get(2..).unwrap_or_default();
        let Some(fin) = body.iter().position(|b| (0x40..=0x7e).contains(b)) else { return Step::Incomplete };
        let params = body.get(..fin).unwrap_or_default();
        let final_byte = body.get(fin).copied().unwrap_or(0);
        let len = 2 + fin + 1;
        if final_byte == b'~' && params == b"200" {
            self.paste = Some(Vec::new());
            return Step::Done(len, None);
        }
        Step::Done(len, csi_input(params, final_byte))
    }
}

/// A plain byte or UTF-8 character at `pending[at..]`, with `mods` (Alt after a leading ESC).
fn plain(pending: &[u8], mods: Mods, at: usize) -> Step {
    let Some(&b) = pending.get(at) else { return Step::Incomplete };
    let chord = |code, extra: Mods| Some(Input::Key(Chord::new(code, mods | extra)));
    let input = match b {
        b'\r' | b'\n' => chord(KeyCode::Enter, Mods::NONE),
        b'\t' => chord(KeyCode::Tab, Mods::NONE),
        0x7f | 0x08 => chord(KeyCode::Backspace, Mods::NONE),
        0x00 => chord(KeyCode::Char(' '), Mods::CTRL),
        0x01..=0x1a => chord(KeyCode::Char(char::from(b'a' + b - 1)), Mods::CTRL),
        0x1c..=0x1f => chord(KeyCode::Char(char::from(b'\\' + b - 0x1c)), Mods::CTRL),
        _ if b < 0x80 => chord(KeyCode::Char(char::from(b)), Mods::NONE),
        _ => {
            let width = utf8_width(b);
            let Some(bytes) = pending.get(at..at + width) else { return Step::Incomplete };
            let Some(c) = std::str::from_utf8(bytes).ok().and_then(|s| s.chars().next()) else {
                return Step::Done(at + 1, None);
            };
            return Step::Done(at + width, chord(KeyCode::Char(c), Mods::NONE));
        }
    };
    Step::Done(at + 1, input)
}

fn utf8_width(lead: u8) -> usize {
    match lead {
        0xc0..=0xdf => 2,
        0xe0..=0xef => 3,
        0xf0..=0xf7 => 4,
        _ => 1,
    }
}

/// `ESC O <final>`: application-mode arrows, Home, End, F1–F4.
fn ss3(pending: &[u8]) -> Step {
    let Some(&fin) = pending.get(2) else { return Step::Incomplete };
    let code = match fin {
        b'A' => KeyCode::Up,
        b'B' => KeyCode::Down,
        b'C' => KeyCode::Right,
        b'D' => KeyCode::Left,
        b'H' => KeyCode::Home,
        b'F' => KeyCode::End,
        b'P'..=b'S' => KeyCode::F(fin - b'P' + 1),
        _ => return Step::Done(3, None),
    };
    Step::Done(3, Some(Input::Key(Chord::plain(code))))
}

/// APC, DCS, OSC, SOS, PM: dropped up to `ESC \` (or BEL for OSC).
fn string(pending: &[u8]) -> Step {
    let st = find(pending, b"\x1b\\").map(|i| i + 2);
    let bel = (pending.get(1) == Some(&b']')).then(|| pending.iter().position(|&b| b == 0x07).map(|i| i + 1)).flatten();
    match st.into_iter().chain(bel).min() {
        Some(n) => {
            debug!(kind = char::from(pending.get(1).copied().unwrap_or(b'?')).to_string(), "dropped a terminal reply");
            Step::Done(n, None)
        }
        None => Step::Incomplete,
    }
}

fn csi_input(params: &[u8], final_byte: u8) -> Option<Input> {
    if params.first() == Some(&b'?') {
        debug!(final_byte = char::from(final_byte).to_string(), "dropped a terminal reply");
        return None;
    }
    if params.first() == Some(&b'<') {
        return sgr_mouse(params.get(1..).unwrap_or_default(), final_byte);
    }
    let text = std::str::from_utf8(params).ok()?;
    let mut fields = text.split(';');
    let first = fields.next().unwrap_or("");
    let second = fields.next().unwrap_or("");
    let key_code = |s: &str| s.split(':').next().and_then(|n| n.parse::<u32>().ok());
    let mods = mods_field(second);
    let chord = |code| Some(Input::Key(Chord::new(code, mods)));
    match final_byte {
        b'u' => kitty(first, second),
        b'A' => chord(KeyCode::Up),
        b'B' => chord(KeyCode::Down),
        b'C' => chord(KeyCode::Right),
        b'D' => chord(KeyCode::Left),
        b'H' => chord(KeyCode::Home),
        b'F' => chord(KeyCode::End),
        b'P'..=b'S' => chord(KeyCode::F(final_byte - b'P' + 1)),
        b'Z' => Some(Input::Key(Chord::plain(KeyCode::BackTab))),
        b'I' => Some(Input::FocusGained),
        b'O' => Some(Input::FocusLost),
        b'~' => tilde(key_code(first)?).and_then(chord),
        b't' | b'c' | b'R' | b'n' | b'y' => {
            debug!(final_byte = char::from(final_byte).to_string(), "dropped a terminal reply");
            None
        }
        _ => None,
    }
}

/// The modifier field `1 + bits[:event]`; missing means none.
fn mods_field(field: &str) -> Mods {
    let value = field.split(':').next().and_then(|n| n.parse::<u16>().ok()).unwrap_or(1);
    Mods::from_bits(u8::try_from(value.saturating_sub(1) & 0xff).unwrap_or(0))
}

fn tilde(code: u32) -> Option<KeyCode> {
    Some(match code {
        1 | 7 => KeyCode::Home,
        2 => KeyCode::Insert,
        3 => KeyCode::Delete,
        4 | 8 => KeyCode::End,
        5 => KeyCode::PageUp,
        6 => KeyCode::PageDown,
        11..=15 => KeyCode::F(u8::try_from(code - 10).ok()?),
        17..=21 => KeyCode::F(u8::try_from(code - 11).ok()?),
        23..=26 => KeyCode::F(u8::try_from(code - 12).ok()?),
        28 | 29 => KeyCode::F(u8::try_from(code - 13).ok()?),
        31..=34 => KeyCode::F(u8::try_from(code - 14).ok()?),
        _ => return None,
    })
}

/// `CSI code[:alternates] ; mods[:event] u`; release events (event 3) are dropped.
fn kitty(first: &str, second: &str) -> Option<Input> {
    let code: u32 = first.split(':').next()?.parse().ok()?;
    if second.split(':').nth(1) == Some("3") {
        return None;
    }
    let mods = mods_field(second);
    let key = match code {
        13 | 57414 => KeyCode::Enter,
        9 => KeyCode::Tab,
        27 => KeyCode::Esc,
        127 => KeyCode::Backspace,
        57_344..=57_454 => return None,
        _ => KeyCode::Char(char::from_u32(code)?),
    };
    Some(Input::Key(Chord::new(key, mods)))
}

/// `CSI < b ; x ; y (M|m)`.
fn sgr_mouse(params: &[u8], final_byte: u8) -> Option<Input> {
    let text = std::str::from_utf8(params).ok()?;
    let mut nums = text.split(';').map(|n| n.parse::<u16>().ok());
    let (b, x, y) = (nums.next()??, nums.next()??, nums.next()??);
    let mut mods = Mods::NONE;
    for (bit, m) in [(4, Mods::SHIFT), (8, Mods::ALT), (16, Mods::CTRL)] {
        if b & bit != 0 {
            mods = mods | m;
        }
    }
    let button = match b & 0b11 {
        0 => Button::Left,
        1 => Button::Middle,
        _ => Button::Right,
    };
    let kind = if b & 64 != 0 {
        if b & 1 == 0 { MouseKind::ScrollUp } else { MouseKind::ScrollDown }
    } else if b & 32 != 0 {
        MouseKind::Drag(button)
    } else if final_byte == b'm' {
        MouseKind::Release(button)
    } else {
        MouseKind::Press(button)
    };
    Some(Input::Mouse(Mouse::new(kind, x.saturating_sub(1), y.saturating_sub(1), mods)))
}

fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}
```

An over-long incomplete sequence is dropped and parsing starts afresh, so later keys work; the rest of a pathological unterminated string may then type as text, which is accepted. `Decoder::csi` takes `&mut self` only because it starts a paste.

- [ ] **Step 5: Gates (`cargo mutants --in-diff` optional here, required before the PR); commit** `feat: input decoder`.

### Task 5: probe query, reply parser and capabilities

**Files:**

- Create: `crates/pawsman-tui/src/term.rs` (module root only for now), `term/tmux.rs`, `term/probe.rs`, `term/caps.rs`

**Interfaces:**

- Consumes: `EnvSnapshot`, `ImageMode`.
- Produces:
  - `term::InTmux::{Yes, No}` with `from_env(&EnvSnapshot)`; `tmux::passthrough(&[u8]) -> Vec<u8>`.
  - `probe::Query { bytes() -> &[u8], da1_needed() -> usize }`, `probe::query(InTmux) -> Query`.
  - `probe::Replies` (`graphics`, `keyboard`, `cell_px: Option<CellPx>`, `da1: Vec<Da1>`, `truecolor`) and `probe::parse(&[u8]) -> Replies`; `probe::Da1 { sixel }`.
  - `term::{Caps, Graphics, KeyboardProtocol, ColorDepth, OverSsh, CellPx, CapsHint}`; `Caps::resolve(&Replies, &EnvSnapshot, ImageMode) -> Caps` and getters `graphics()`, `keyboard()`, `color()`, `cell_px()`, `tmux()`, `ssh()`, `hint()`.

- [ ] **Step 1: Failing tests**

`probe.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn outside_tmux_one_da1_ends_the_probe() {
        let q = query(InTmux::No);

        assert_eq!(q.da1_needed(), 1, "one DA1");
        assert!(q.bytes().ends_with(b"\x1b[c"), "DA1 last");
        assert!(q.bytes().starts_with(b"\x1b_Gi=31,s=1,v=1,a=q,t=d,f=24;AAAA\x1b\\"), "graphics query first");
    }

    #[test]
    fn inside_tmux_the_graphics_query_and_a_second_da1_pass_through() {
        let q = query(InTmux::Yes);

        assert_eq!(q.da1_needed(), 2, "tmux's DA1 plus the outer one");
        assert!(q.bytes().ends_with(b"\x1bPtmux;\x1b\x1b[c\x1b\\"), "outer DA1 through passthrough");
        assert!(find(q.bytes(), b"\x1bPtmux;\x1b\x1b_G").is_some(), "graphics query through passthrough");
    }

    #[test]
    fn kitty_answers_everything() {
        let r = parse(b"\x1b_Gi=31;OK\x1b\\\x1b[?1u\x1b[6;20;10t\x1bP1$r48:2:1:2:3m\x1b\\\x1b[?62;22c");

        assert!(r.graphics && r.keyboard && r.truecolor, "graphics, keyboard, truecolor: {r:?}");
        assert_eq!(r.cell_px, Some(CellPx::new(10, 20)), "cell size");
        assert_eq!(r.da1, vec![Da1 { sixel: false }], "one DA1 without sixel");
    }

    #[test]
    fn graphics_error_is_not_support() {
        let r = parse(b"\x1b_Gi=31;ENOTSUPPORTED:no\x1b\\\x1b[?62c");

        assert!(!r.graphics, "only OK counts");
    }

    #[test]
    fn da1_replies_keep_their_order() {
        let r = parse(b"\x1b[?1;2;4c\x1b_Gi=31;OK\x1b\\\x1b[?62;22c");

        assert_eq!(r.da1, vec![Da1 { sixel: true }, Da1 { sixel: false }], "tmux first, outer second");
    }

    #[test]
    fn decrqss_with_semicolons_counts_as_truecolor() {
        assert!(parse(b"\x1bP1$r48;2;1;2;3m\x1b\\").truecolor, "semicolon form");
        assert!(!parse(b"\x1bP0$r\x1b\\").truecolor, "invalid request");
    }

    #[test]
    fn garbage_and_truncation_never_panic() {
        for bytes in [&b"\x1b"[..], b"\x1b[", b"\x1b_G", b"\x1b[6;x;yt", b"\x1bP1$r", b"\xff\xfe"] {
            let _ = parse(bytes);
        }
    }
}
```

`caps.rs`:

```rust
#[cfg(test)]
mod tests {
    use pawsman_core::env::EnvSnapshot;
    use pawsman_core::settings::ImageMode;
    use rstest::rstest;

    use super::*;
    use crate::term::probe::parse;

    const KITTY: &[u8] = b"\x1b_Gi=31;OK\x1b\\\x1b[?1u\x1b[6;20;10t\x1b[?62;22c";
    const TMUX_PASSTHROUGH: &[u8] = b"\x1b[?1;2;4c\x1b_Gi=31;OK\x1b\\\x1b[?62;22c";
    const TMUX_NO_PASSTHROUGH: &[u8] = b"\x1b[?1;2;4c";
    const XTERM: &[u8] = b"\x1b[?63;1;2;4;6;9;15;22c";

    fn env(pairs: &[(&str, &str)]) -> EnvSnapshot {
        EnvSnapshot::from_pairs(pairs.iter().copied())
    }

    const UTF8: (&str, &str) = ("LANG", "C.UTF-8");
    const TMUX: (&str, &str) = ("TMUX", "/tmp/tmux-1000/default,1,0");

    #[rstest]
    #[case::kitty(KITTY, &[UTF8, ("TERM", "xterm-kitty")], Graphics::Kitty, None)]
    #[case::tmux_passthrough(TMUX_PASSTHROUGH, &[UTF8, TMUX], Graphics::Kitty, None)]
    #[case::tmux_sixel_claim_ignored(TMUX_NO_PASSTHROUGH, &[UTF8, TMUX], Graphics::Blocks, Some(CapsHint::TmuxPassthrough))]
    #[case::tmux_without_utf8(TMUX_PASSTHROUGH, &[("LANG", "C"), TMUX], Graphics::Off, Some(CapsHint::TmuxNeedsUtf8))]
    #[case::xterm_sixel(XTERM, &[UTF8, ("TERM", "xterm")], Graphics::Sixel, None)]
    #[case::silence_utf8(b"", &[UTF8], Graphics::Blocks, None)]
    #[case::silence_no_utf8(b"", &[], Graphics::Off, None)]
    #[case::dumb(KITTY, &[UTF8, ("TERM", "dumb")], Graphics::Off, None)]
    fn auto_graphics_follow_the_probe(
        #[case] replies: &[u8],
        #[case] pairs: &[(&str, &str)],
        #[case] graphics: Graphics,
        #[case] hint: Option<CapsHint>,
    ) {
        let caps = Caps::resolve(&parse(replies), &env(pairs), ImageMode::Auto);

        assert_eq!(caps.graphics(), graphics, "graphics for {pairs:?}");
        assert_eq!(caps.hint(), hint, "hint for {pairs:?}");
    }

    #[test]
    fn an_explicit_mode_wins() {
        let caps = Caps::resolve(&parse(b""), &env(&[]), ImageMode::Kitty);

        assert_eq!(caps.graphics(), Graphics::Kitty, "forced kitty");
    }

    #[rstest]
    #[case::colorterm(&[("COLORTERM", "truecolor"), ("TERM", "xterm")], b"", ColorDepth::TrueColor)]
    #[case::decrqss(&[("TERM", "xterm")], b"\x1bP1$r48:2:1:2:3m\x1b\\", ColorDepth::TrueColor)]
    #[case::term_256(&[("TERM", "xterm-256color")], b"", ColorDepth::Indexed256)]
    #[case::tmux(&[TMUX, ("TERM", "screen")], b"", ColorDepth::Indexed256)]
    #[case::plain(&[("TERM", "xterm")], b"", ColorDepth::Ansi16)]
    #[case::dumb(&[("TERM", "dumb"), ("COLORTERM", "truecolor")], b"", ColorDepth::Ansi16)]
    fn color_depth(#[case] pairs: &[(&str, &str)], #[case] replies: &[u8], #[case] expected: ColorDepth) {
        let caps = Caps::resolve(&parse(replies), &env(pairs), ImageMode::Auto);

        assert_eq!(caps.color(), expected, "colour for {pairs:?}");
    }

    #[test]
    fn keyboard_comes_from_the_probe_only() {
        let kitty = Caps::resolve(&parse(KITTY), &env(&[]), ImageMode::Auto);
        let tmux = Caps::resolve(&parse(TMUX_PASSTHROUGH), &env(&[TMUX]), ImageMode::Auto);

        assert_eq!(kitty.keyboard(), KeyboardProtocol::Kitty, "kitty answers CSI ? u");
        assert_eq!(tmux.keyboard(), KeyboardProtocol::Legacy, "tmux does not");
    }
}
```

`tmux.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn passthrough_doubles_every_escape() {
        assert_eq!(passthrough(b"\x1b[c"), b"\x1bPtmux;\x1b\x1b[c\x1b\\".to_vec(), "wrapped");
    }
}
```

- [ ] **Step 2: Run; see failures; commit** `test: probe query, replies and capabilities`.

- [ ] **Step 3: Implement**

`term/tmux.rs`:

```rust
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
    let mut out = b"\x1bPtmux;".to_vec();
    for &b in seq {
        if b == 0x1b {
            out.push(0x1b);
        }
        out.push(b);
    }
    out.extend_from_slice(b"\x1b\\");
    out
}
```

`term/probe.rs` (pure part; the reader comes in Task 7):

```rust
//! The capability probe: query bytes, and a pure parser for the replies (order does not matter).

use super::caps::CellPx;
use super::tmux::{InTmux, passthrough};

const GRAPHICS_QUERY: &[u8] = b"\x1b_Gi=31,s=1,v=1,a=q,t=d,f=24;AAAA\x1b\\";
/// Keyboard flags, cell size in pixels, and an SGR truecolour background read back by DECRQSS.
const PLAIN_QUERY: &[u8] = b"\x1b[?u\x1b[16t\x1b[48:2:1:2:3m\x1bP$qm\x1b\\\x1b[m";
const DA1: &[u8] = b"\x1b[c";

/// What to write and how many DA1 replies end the probe.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Query {
    bytes: Vec<u8>,
    da1_needed: usize,
}

impl Query {
    /// The bytes to write.
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// DA1 replies that end the probe.
    #[must_use]
    pub fn da1_needed(&self) -> usize {
        self.da1_needed
    }
}

/// The probe for this terminal. Inside tmux, tmux answers the plain DA1 itself, so the graphics
/// query and a second DA1 go through passthrough; the outer terminal answers them in order.
#[must_use]
pub fn query(tmux: InTmux) -> Query {
    match tmux {
        InTmux::No => Query { bytes: [GRAPHICS_QUERY, PLAIN_QUERY, DA1].concat(), da1_needed: 1 },
        InTmux::Yes => Query {
            bytes: [PLAIN_QUERY, DA1, &passthrough(GRAPHICS_QUERY), &passthrough(DA1)].concat(),
            da1_needed: 2,
        },
    }
}

/// One DA1 reply.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Da1 {
    /// Attribute 4: sixel graphics.
    pub sixel: bool,
}

/// Everything the replies said.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Replies {
    /// The kitty graphics query got `OK`.
    pub graphics: bool,
    /// `CSI ? flags u` arrived: the kitty keyboard protocol.
    pub keyboard: bool,
    /// `CSI 6 ; h ; w t`.
    pub cell_px: Option<CellPx>,
    /// DA1 replies in arrival order.
    pub da1: Vec<Da1>,
    /// DECRQSS read the truecolour background back.
    pub truecolor: bool,
}

/// Parses whatever replies arrived; unknown or truncated bytes are skipped.
#[must_use]
pub fn parse(bytes: &[u8]) -> Replies {
    let mut replies = Replies::default();
    let mut rest = bytes;
    while let Some(start) = rest.iter().position(|&b| b == 0x1b) {
        rest = rest.get(start..).unwrap_or_default();
        let consumed = match rest.get(1) {
            Some(b'_') => apc(rest, &mut replies),
            Some(b'P') => dcs(rest, &mut replies),
            Some(b'[') => csi(rest, &mut replies),
            _ => 1,
        };
        rest = rest.get(consumed.max(1)..).unwrap_or_default();
    }
    replies
}

fn string_end(rest: &[u8]) -> usize {
    find(rest, b"\x1b\\").map_or(rest.len(), |i| i + 2)
}

fn apc(rest: &[u8], replies: &mut Replies) -> usize {
    let end = string_end(rest);
    let body = rest.get(2..end.saturating_sub(2)).unwrap_or_default();
    if body.first() == Some(&b'G') && body.ends_with(b";OK") && find(body, b"i=31").is_some() {
        replies.graphics = true;
    }
    end
}

fn dcs(rest: &[u8], replies: &mut Replies) -> usize {
    let end = string_end(rest);
    let body = rest.get(2..end.saturating_sub(2)).unwrap_or_default();
    if body.starts_with(b"1$r") && (find(body, b"48:2:1:2:3").is_some() || find(body, b"48;2;1;2;3").is_some()) {
        replies.truecolor = true;
    }
    end
}

fn csi(rest: &[u8], replies: &mut Replies) -> usize {
    let body = rest.get(2..).unwrap_or_default();
    let Some(fin) = body.iter().position(|b| (0x40..=0x7e).contains(b)) else { return rest.len() };
    let params = body.get(..fin).unwrap_or_default();
    match body.get(fin) {
        Some(b'u') if params.first() == Some(&b'?') => replies.keyboard = true,
        Some(b'c') if params.first() == Some(&b'?') => {
            let sixel = params.get(1..).unwrap_or_default().split(|&b| b == b';').any(|a| a == b"4");
            replies.da1.push(Da1 { sixel });
        }
        Some(b't') => {
            let nums: Vec<u16> =
                std::str::from_utf8(params).unwrap_or("").split(';').filter_map(|n| n.parse().ok()).collect();
            if let [6, h, w] = nums.as_slice() {
                replies.cell_px = Some(CellPx::new(*w, *h));
            }
        }
        _ => {}
    }
    2 + fin + 1
}

pub(super) fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}
```

`term/caps.rs`:

```rust
//! Terminal capabilities, resolved once from the probe replies and the env snapshot. Views branch
//! on these, never on `$TERM`.

use pawsman_core::env::EnvSnapshot;
use pawsman_core::settings::ImageMode;

use super::probe::Replies;
use super::tmux::InTmux;

/// How images are drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Graphics {
    /// Kitty graphics protocol through Unicode placeholders.
    Kitty,
    /// Sixel.
    Sixel,
    /// Half blocks.
    Blocks,
    /// A text placeholder.
    Off,
}

/// Which key encoding the terminal speaks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyboardProtocol {
    /// The kitty keyboard protocol (disambiguate flag).
    Kitty,
    /// Legacy escape sequences.
    Legacy,
}

/// Colours the terminal shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorDepth {
    /// 24-bit RGB.
    TrueColor,
    /// The xterm 256-colour palette.
    Indexed256,
    /// The 16 ANSI colours.
    Ansi16,
}

/// Whether the session runs over SSH.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverSsh {
    /// `SSH_TTY` or `SSH_CONNECTION` is set.
    Yes,
    /// Local.
    No,
}

/// A cell's size in pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CellPx {
    width: u16,
    height: u16,
}

impl CellPx {
    /// A cell size.
    #[must_use]
    pub const fn new(width: u16, height: u16) -> Self {
        Self { width, height }
    }

    /// Width in pixels.
    #[must_use]
    pub const fn width(self) -> u16 {
        self.width
    }

    /// Height in pixels.
    #[must_use]
    pub const fn height(self) -> u16 {
        self.height
    }
}

/// Something the user can change to get more.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CapsHint {
    /// tmux without a UTF-8 locale rewrites image placeholders and half blocks.
    TmuxNeedsUtf8,
    /// tmux did not pass the graphics query through.
    TmuxPassthrough,
}

impl CapsHint {
    /// The text the hint bar shows.
    #[must_use]
    pub fn text(self) -> &'static str {
        match self {
            Self::TmuxNeedsUtf8 => "images off: run tmux with -u",
            Self::TmuxPassthrough => "images: set allow-passthrough on in tmux",
        }
    }
}

/// The resolved capabilities.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Caps {
    graphics: Graphics,
    keyboard: KeyboardProtocol,
    color: ColorDepth,
    cell_px: Option<CellPx>,
    tmux: InTmux,
    ssh: OverSsh,
    hint: Option<CapsHint>,
}

impl Caps {
    /// Resolves the capabilities; an explicit `images` mode wins over the probe.
    #[must_use]
    pub fn resolve(replies: &Replies, env: &EnvSnapshot, images: ImageMode) -> Self {
        let tmux = InTmux::from_env(env);
        let dumb = env.term() == Some("dumb");
        let utf8 = env.utf8_locale();
        let outer_da1 = match tmux {
            InTmux::Yes => replies.da1.get(1),
            InTmux::No => replies.da1.first(),
        };
        let sixel = outer_da1.is_some_and(|d| d.sixel);
        let mut hint = None;
        let graphics = match images {
            ImageMode::Kitty => Graphics::Kitty,
            ImageMode::Sixel => Graphics::Sixel,
            ImageMode::Blocks => Graphics::Blocks,
            ImageMode::Off => Graphics::Off,
            ImageMode::Auto if dumb => Graphics::Off,
            ImageMode::Auto if tmux == InTmux::Yes && !utf8 => {
                hint = Some(CapsHint::TmuxNeedsUtf8);
                Graphics::Off
            }
            ImageMode::Auto if replies.graphics => Graphics::Kitty,
            ImageMode::Auto => {
                if tmux == InTmux::Yes {
                    hint = Some(CapsHint::TmuxPassthrough);
                }
                if sixel {
                    Graphics::Sixel
                } else if utf8 {
                    Graphics::Blocks
                } else {
                    Graphics::Off
                }
            }
        };
        let color = if dumb {
            ColorDepth::Ansi16
        } else if env.truecolor() || replies.truecolor {
            ColorDepth::TrueColor
        } else if tmux == InTmux::Yes || env.term().is_some_and(|t| t.contains("256")) {
            ColorDepth::Indexed256
        } else {
            ColorDepth::Ansi16
        };
        Self {
            graphics,
            keyboard: if replies.keyboard { KeyboardProtocol::Kitty } else { KeyboardProtocol::Legacy },
            color,
            cell_px: replies.cell_px,
            tmux,
            ssh: if env.over_ssh() { OverSsh::Yes } else { OverSsh::No },
            hint,
        }
    }

    /// Image mode.
    #[must_use]
    pub fn graphics(&self) -> Graphics {
        self.graphics
    }

    /// Key encoding.
    #[must_use]
    pub fn keyboard(&self) -> KeyboardProtocol {
        self.keyboard
    }

    /// Colour depth.
    #[must_use]
    pub fn color(&self) -> ColorDepth {
        self.color
    }

    /// Cell size, when the terminal reported it.
    #[must_use]
    pub fn cell_px(&self) -> Option<CellPx> {
        self.cell_px
    }

    /// Inside tmux.
    #[must_use]
    pub fn tmux(&self) -> InTmux {
        self.tmux
    }

    /// Over SSH.
    #[must_use]
    pub fn ssh(&self) -> OverSsh {
        self.ssh
    }

    /// A hint for the hint bar.
    #[must_use]
    pub fn hint(&self) -> Option<CapsHint> {
        self.hint
    }
}
```

`term.rs` for now:

```rust
//! Terminal ownership: modes, the capability probe and capabilities.

mod caps;
pub mod probe;
mod tmux;

pub use caps::{Caps, CapsHint, CellPx, ColorDepth, Graphics, KeyboardProtocol, OverSsh};
pub use tmux::{InTmux, passthrough};
```

`lib.rs` adds `pub mod term;`.

- [ ] **Step 4: Gates; commit** `feat: probe query, reply parser and capabilities`.

### Task 6: TUI errors, the mode stack and the panic hook

**Files:**

- Create: `crates/pawsman-tui/src/{error,codes}.rs`, `term/modes.rs`, `docs/errors/tui/{no_tty,terminal_mode,terminal_io,input_closed}.md`
- Modify: `term.rs`, `lib.rs`

**Interfaces:**

- Produces:
  - `TuiError` (`ErrorInfo`; `pub(crate)` constructors `no_tty(io::Error)`, `terminal_mode(Mode, io::Error)`, `terminal_io(io::Error)`, `input_closed()`), `codes::{NO_TTY, TERMINAL_MODE, TERMINAL_IO, INPUT_CLOSED, ALL}`.
  - `modes::Mode::{Raw, AltScreen, HiddenCursor, KittyKeyboard, BracketedPaste, Focus, Mouse}` with `on() -> &'static [u8]`, `off() -> &'static [u8]`, `name() -> &'static str`; `modes::MODES: &[Mode]` (setup order).
  - `modes::Stack` (`new(Box<dyn Write + Send>)`, `with_raw(fd: OwnedFd, saved: Termios)`, `push(Mode)`, `undo_all()`, `active() -> &[Mode]`) and the global functions `modes::install(Stack)`, `modes::push(Mode)`, `modes::restore()`, `modes::restore_from_panic()`, `modes::install_panic_hook()`.

- [ ] **Step 1: Failing tests** (`modes.rs`; stub `undo_all` does nothing):

```rust
#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::*;

    #[derive(Clone, Default)]
    struct Shared(Arc<Mutex<Vec<u8>>>);

    impl std::io::Write for Shared {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn modes_are_undone_newest_first() {
        let out = Shared::default();
        let mut stack = Stack::new(Box::new(out.clone()));
        for mode in [Mode::AltScreen, Mode::HiddenCursor, Mode::KittyKeyboard, Mode::BracketedPaste, Mode::Focus, Mode::Mouse] {
            stack.push(mode);
        }

        stack.undo_all();

        let bytes = out.0.lock().unwrap().clone();
        assert_eq!(
            bytes,
            b"\x1b[?1006l\x1b[?1002l\x1b[?1000l\x1b[?1004l\x1b[?2004l\x1b[<u\x1b[?25h\x1b[?1049l".to_vec(),
            "mouse, focus, paste, keyboard, cursor, alternate screen"
        );
        assert!(stack.active().is_empty(), "nothing left");
    }

    #[test]
    fn undo_twice_writes_once() {
        let out = Shared::default();
        let mut stack = Stack::new(Box::new(out.clone()));
        stack.push(Mode::AltScreen);

        stack.undo_all();
        stack.undo_all();

        assert_eq!(out.0.lock().unwrap().as_slice(), b"\x1b[?1049l", "idempotent");
    }

    #[test]
    fn mouse_never_enables_any_motion() {
        assert!(!Mode::Mouse.on().windows(4).any(|w| w == b"1003"), "no any-motion tracking");
        assert_eq!(Mode::KittyKeyboard.on(), b"\x1b[>1u", "disambiguate only");
    }

    #[test]
    fn every_mode_has_an_undo() {
        for mode in MODES {
            assert!(*mode == Mode::Raw || !mode.off().is_empty(), "{mode:?} has an off sequence");
        }
    }
}
```

`codes.rs` registry test as in Task 2 with namespace `"tui"`.

- [ ] **Step 2: Run; see failures; commit** `test: mode stack and tui error registry`.

- [ ] **Step 3: Implement**

`term/modes.rs`:

```rust
//! Every terminal mode pawsman turns on, and the one place that undoes them, newest first: on exit,
//! on error, from `Drop` and from the panic hook (rules/tui.md §2).

use std::io::Write;
use std::os::fd::OwnedFd;
use std::sync::{Mutex, TryLockError};

use rustix::termios::{OptionalActions, Termios, tcsetattr};

/// One terminal mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Raw input through termios; undone by restoring the saved termios.
    Raw,
    /// The alternate screen.
    AltScreen,
    /// The cursor is hidden.
    HiddenCursor,
    /// Kitty keyboard protocol, disambiguate flag only.
    KittyKeyboard,
    /// Bracketed paste.
    BracketedPaste,
    /// Focus events.
    Focus,
    /// Button-event mouse tracking with SGR coordinates; never any-motion (1003).
    Mouse,
}

/// The modes in setup order.
pub const MODES: &[Mode] =
    &[Mode::Raw, Mode::AltScreen, Mode::HiddenCursor, Mode::KittyKeyboard, Mode::BracketedPaste, Mode::Focus, Mode::Mouse];

impl Mode {
    /// The bytes that turn the mode on; empty for `Raw`.
    #[must_use]
    pub const fn on(self) -> &'static [u8] {
        match self {
            Self::Raw => b"",
            Self::AltScreen => b"\x1b[?1049h",
            Self::HiddenCursor => b"\x1b[?25l",
            Self::KittyKeyboard => b"\x1b[>1u",
            Self::BracketedPaste => b"\x1b[?2004h",
            Self::Focus => b"\x1b[?1004h",
            Self::Mouse => b"\x1b[?1000h\x1b[?1002h\x1b[?1006h",
        }
    }

    /// The bytes that turn it off; empty for `Raw`.
    #[must_use]
    pub const fn off(self) -> &'static [u8] {
        match self {
            Self::Raw => b"",
            Self::AltScreen => b"\x1b[?1049l",
            Self::HiddenCursor => b"\x1b[?25h",
            Self::KittyKeyboard => b"\x1b[<u",
            Self::BracketedPaste => b"\x1b[?2004l",
            Self::Focus => b"\x1b[?1004l",
            Self::Mouse => b"\x1b[?1006l\x1b[?1002l\x1b[?1000l",
        }
    }

    /// A name for errors and logs.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Raw => "raw",
            Self::AltScreen => "alternate screen",
            Self::HiddenCursor => "hidden cursor",
            Self::KittyKeyboard => "kitty keyboard",
            Self::BracketedPaste => "bracketed paste",
            Self::Focus => "focus events",
            Self::Mouse => "mouse",
        }
    }
}

/// Active modes, the terminal to write their undo to, and the termios to restore.
pub struct Stack {
    out: Box<dyn Write + Send>,
    raw: Option<(OwnedFd, Termios)>,
    active: Vec<Mode>,
}

impl Stack {
    /// A stack writing undo sequences to `out`.
    #[must_use]
    pub fn new(out: Box<dyn Write + Send>) -> Self {
        Self { out, raw: None, active: Vec::new() }
    }

    /// Remembers the termios to restore when `Raw` is undone.
    #[must_use]
    pub fn with_raw(mut self, fd: OwnedFd, saved: Termios) -> Self {
        self.raw = Some((fd, saved));
        self
    }

    /// Records that `mode` is on.
    pub fn push(&mut self, mode: Mode) {
        self.active.push(mode);
    }

    /// The modes that are on, oldest first.
    #[must_use]
    pub fn active(&self) -> &[Mode] {
        &self.active
    }

    /// Undoes every active mode, newest first; errors are ignored, since this runs on the way out.
    pub fn undo_all(&mut self) {
        while let Some(mode) = self.active.pop() {
            if mode == Mode::Raw {
                if let Some((fd, saved)) = &self.raw {
                    let _ = tcsetattr(fd, OptionalActions::Now, saved);
                }
            } else {
                let _ = self.out.write_all(mode.off());
            }
        }
        let _ = self.out.flush();
    }
}

static STACK: Mutex<Option<Stack>> = Mutex::new(None);

/// Makes `stack` the process-wide stack that `restore` and the panic hook undo.
pub fn install(stack: Stack) {
    *STACK.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = Some(stack);
}

/// Records `mode` on the process-wide stack.
pub fn push(mode: Mode) {
    if let Some(stack) = STACK.lock().unwrap_or_else(std::sync::PoisonError::into_inner).as_mut() {
        stack.push(mode);
    }
}

/// Undoes every mode. Safe to call more than once.
pub fn restore() {
    if let Some(stack) = STACK.lock().unwrap_or_else(std::sync::PoisonError::into_inner).as_mut() {
        stack.undo_all();
    }
}

/// Like `restore`, but never blocks: a panic while the stack is locked leaves it as it is.
pub fn restore_from_panic() {
    let mut guard = match STACK.try_lock() {
        Ok(guard) => guard,
        Err(TryLockError::Poisoned(poisoned)) => poisoned.into_inner(),
        Err(TryLockError::WouldBlock) => return,
    };
    if let Some(stack) = guard.as_mut() {
        stack.undo_all();
    }
}

/// Restores the terminal before the default panic message, on the main thread only: a worker's
/// panic ends its thread, and the main loop sees the closed channel and exits cleanly.
pub fn install_panic_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        tracing::error!(panic = %info, "panic");
        if std::thread::current().name() == Some("main") {
            restore_from_panic();
        }
        previous(info);
    }));
}
```

`error.rs` and `codes.rs` follow Task 2's shape:

| Kind | Code | Message | Help |
| --- | --- | --- | --- |
| `NoTty(io::Error)` | `tui::no_tty` | ``cannot open `/dev/tty`: {err}`` | ``Run pawsman in a terminal; over SSH use `ssh -t`.`` |
| `TerminalMode { mode: &'static str, source }` | `tui::terminal_mode` | `terminal refused {mode} mode: {err}` | `Run pawsman in a full terminal emulator.` |
| `TerminalIo(io::Error)` | `tui::terminal_io` | `terminal write: {err}` | none |
| `InputClosed` | `tui::input_closed` | `input reader stopped` | none |

`TuiError::terminal_mode(mode: Mode, err)` stores `mode.name()`. `std::error::Error::source` returns the inner `io::Error` where there is one. Pages follow the template; `no_tty.md` names cron, `ssh` without `-t` and piped input as the usual causes.

`term.rs` adds `pub(crate) mod modes;`; `lib.rs` adds `pub mod codes; mod error; pub use error::TuiError;`.

- [ ] **Step 4: Gates; commit** `feat: mode stack, panic hook and tui errors`.

### Task 7: the terminal and the probe reader

**Files:**

- Modify: `term.rs`, `term/probe.rs`

**Interfaces:**

- Consumes: `Mode`, `modes::*`, `Query`, `TuiError`, `KeyboardProtocol`, `InTmux`.
- Produces:
  - `probe::Timeouts { reply, grace, tmux_grace }` with `Timeouts::DEFAULT` (1 s, 150 ms, 500 ms); `probe::read_replies<F: Read + AsFd>(tty: &mut F, query: &Query, timeouts: Timeouts) -> io::Result<Probed>`; `Probed { bytes: Vec<u8>, took: Duration }`.
  - `term::Term::open() -> Result<Term, TuiError>` (opens `/dev/tty`, saves termios, enters raw mode, alternate screen, hides the cursor, installs the stack and the panic hook); `Term::probe(&mut self, InTmux) -> Result<Probed, TuiError>`; `Term::enable_input(&mut self, KeyboardProtocol)` (kitty flags when supported, paste, focus, mouse; a mode that fails is logged and skipped); `Term::reader(&self) -> Result<File, TuiError>` (a dup of the tty for the input thread); `Term::writer(&self) -> Result<File, TuiError>` (a dup for the ratatui backend); `Term::restore(&mut self)`; `Drop` restores.

- [ ] **Step 1: Failing tests** in `probe.rs` (stub `read_replies` returns empty bytes at once):

```rust
    use std::io::Write as _;
    use std::os::unix::net::UnixStream;
    use std::time::{Duration, Instant};

    fn fast() -> Timeouts {
        Timeouts { reply: Duration::from_millis(300), grace: Duration::from_millis(50), tmux_grace: Duration::from_millis(120) }
    }

    #[test]
    fn reading_stops_at_the_da1() {
        let (mut ours, mut theirs) = UnixStream::pair().unwrap();
        theirs.write_all(b"\x1b_Gi=31;OK\x1b\\\x1b[?62c").unwrap();

        let probed = read_replies(&mut ours, &query(InTmux::No), fast()).unwrap();

        assert!(parse(&probed.bytes).graphics, "graphics reply read");
        assert!(probed.took < Duration::from_millis(100), "no waiting after DA1: {:?}", probed.took);
    }

    #[test]
    fn silence_times_out_after_reply_plus_grace_and_swallows_late_bytes() {
        let (mut ours, mut theirs) = UnixStream::pair().unwrap();
        let late = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(320));
            theirs.write_all(b"\x1b_Gi=31;OK\x1b\\").unwrap();
            theirs
        });

        let start = Instant::now();
        let probed = read_replies(&mut ours, &query(InTmux::No), fast()).unwrap();
        let _theirs = late.join().unwrap();

        assert!(start.elapsed() >= Duration::from_millis(340), "waited reply + grace: {:?}", start.elapsed());
        assert!(parse(&probed.bytes).graphics, "the late reply landed in the probe, not the input");
    }

    #[test]
    fn inside_tmux_a_missing_second_da1_costs_only_the_tmux_grace() {
        let (mut ours, mut theirs) = UnixStream::pair().unwrap();
        theirs.write_all(b"\x1b[?1;2;4c").unwrap();

        let probed = read_replies(&mut ours, &query(InTmux::Yes), fast()).unwrap();

        assert!(probed.took >= Duration::from_millis(110), "waited for the outer DA1: {:?}", probed.took);
        assert!(probed.took < Duration::from_millis(250), "but not the full timeout: {:?}", probed.took);
    }

    #[test]
    fn inside_tmux_both_da1_end_it_at_once() {
        let (mut ours, mut theirs) = UnixStream::pair().unwrap();
        theirs.write_all(b"\x1b[?1;2;4c\x1b_Gi=31;OK\x1b\\\x1b[?62;22c").unwrap();

        let probed = read_replies(&mut ours, &query(InTmux::Yes), fast()).unwrap();

        assert!(probed.took < Duration::from_millis(100), "outer DA1 ends it: {:?}", probed.took);
    }
```

- [ ] **Step 2: Run; see failures; commit** `test: probe reader timing`.

- [ ] **Step 3: Implement** the reader in `probe.rs`:

```rust
/// How long the probe waits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Timeouts {
    /// For all replies.
    pub reply: Duration,
    /// After the timeout, to swallow late replies.
    pub grace: Duration,
    /// Inside tmux, for the outer DA1 after tmux's own.
    pub tmux_grace: Duration,
}

impl Timeouts {
    /// 1 s, 150 ms, 500 ms ([kitty graphics research](../../../../docs/research/kitty-graphics-over-ssh.md)).
    pub const DEFAULT: Self =
        Self { reply: Duration::from_secs(1), grace: Duration::from_millis(150), tmux_grace: Duration::from_millis(500) };
}

/// What the probe read and how long it took.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Probed {
    /// Every reply byte.
    pub bytes: Vec<u8>,
    /// From the first read to the end.
    pub took: Duration,
}

/// Reads replies until `query.da1_needed()` DA1 replies arrived. On a timeout it keeps reading for
/// the grace period so a late reply does not reach the input; inside tmux, once tmux's own DA1 is
/// in, it waits at most `tmux_grace` for the outer one.
///
/// # Errors
///
/// Any I/O error from `poll` or `read`.
pub fn read_replies<F: Read + AsFd>(tty: &mut F, query: &Query, timeouts: Timeouts) -> io::Result<Probed> {
    let start = Instant::now();
    let mut bytes = Vec::new();
    let mut deadline = start + timeouts.reply;
    let mut in_grace = false;
    let mut short_wait = false;
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            if in_grace || short_wait {
                break;
            }
            in_grace = true;
            deadline = Instant::now() + timeouts.grace;
            continue;
        }
        if !poll_in(tty, left)? {
            continue;
        }
        let mut chunk = [0u8; 1024];
        let n = tty.read(&mut chunk)?;
        if n == 0 {
            break;
        }
        bytes.extend_from_slice(chunk.get(..n).unwrap_or_default());
        let seen = parse(&bytes).da1.len();
        if seen >= query.da1_needed() {
            break;
        }
        if seen >= 1 && query.da1_needed() > 1 && !short_wait {
            short_wait = true;
            deadline = deadline.min(Instant::now() + timeouts.tmux_grace);
        }
    }
    Ok(Probed { bytes, took: start.elapsed() })
}

/// Waits up to `timeout` for `fd` to be readable.
pub(crate) fn poll_in(fd: &impl AsFd, timeout: Duration) -> io::Result<bool> {
    let ts = Timespec {
        tv_sec: i64::try_from(timeout.as_secs()).unwrap_or(i64::MAX),
        tv_nsec: i64::from(timeout.subsec_nanos()),
    };
    let mut fds = [PollFd::new(fd, PollFlags::IN)];
    match poll(&mut fds, Some(&ts)) {
        Ok(n) => Ok(n > 0),
        Err(rustix::io::Errno::INTR) => Ok(false),
        Err(err) => Err(err.into()),
    }
}
```

Imports: `std::io::{self, Read}`, `std::os::fd::AsFd`, `std::time::{Duration, Instant}`, `rustix::event::{PollFd, PollFlags, Timespec, poll}`.

`Term` in `term.rs`:

```rust
/// The terminal, owned by the main loop.
pub struct Term {
    tty: File,
}

impl Term {
    /// Opens `/dev/tty` and enters raw mode, the alternate screen and a hidden cursor; installs the
    /// panic hook. Any failure undoes what was done.
    ///
    /// # Errors
    ///
    /// `tui::no_tty` when there is no terminal; `tui::terminal_mode` when raw mode or the alternate
    /// screen cannot be set.
    pub fn open() -> Result<Self, TuiError> {
        let tty = OpenOptions::new().read(true).write(true).open("/dev/tty").map_err(TuiError::no_tty)?;
        let saved = tcgetattr(&tty).map_err(|e| TuiError::terminal_mode(Mode::Raw, e.into()))?;
        let fd = tty.as_fd().try_clone_to_owned().map_err(TuiError::no_tty)?;
        let out = tty.try_clone().map_err(TuiError::no_tty)?;
        modes::install(Stack::new(Box::new(out)).with_raw(fd, saved.clone()));
        modes::install_panic_hook();
        let mut raw = saved;
        raw.make_raw();
        tcsetattr(&tty, OptionalActions::Now, &raw).map_err(|e| TuiError::terminal_mode(Mode::Raw, e.into()))?;
        modes::push(Mode::Raw);
        let mut term = Self { tty };
        for mode in [Mode::AltScreen, Mode::HiddenCursor] {
            if let Err(err) = term.enable(mode) {
                modes::restore();
                return Err(err);
            }
        }
        Ok(term)
    }

    fn enable(&mut self, mode: Mode) -> Result<(), TuiError> {
        self.tty.write_all(mode.on()).and_then(|()| self.tty.flush()).map_err(|e| TuiError::terminal_mode(mode, e))?;
        modes::push(mode);
        Ok(())
    }

    /// Writes the probe and reads the replies.
    ///
    /// # Errors
    ///
    /// `tui::terminal_io` on a write or read failure.
    pub fn probe(&mut self, tmux: InTmux) -> Result<Probed, TuiError> {
        let query = probe::query(tmux);
        self.tty.write_all(query.bytes()).and_then(|()| self.tty.flush()).map_err(TuiError::terminal_io)?;
        probe::read_replies(&mut self.tty, &query, Timeouts::DEFAULT).map_err(TuiError::terminal_io)
    }

    /// Turns on the input modes: kitty keyboard flags when the probe found them, bracketed paste,
    /// focus events and mouse buttons. A mode that fails is logged and skipped.
    pub fn enable_input(&mut self, keyboard: KeyboardProtocol) {
        let kitty = (keyboard == KeyboardProtocol::Kitty).then_some(Mode::KittyKeyboard);
        for mode in kitty.into_iter().chain([Mode::BracketedPaste, Mode::Focus, Mode::Mouse]) {
            if let Err(err) = self.enable(mode) {
                tracing::warn!(mode = mode.name(), %err, "optional terminal mode skipped");
            }
        }
    }

    /// A handle for the input thread.
    ///
    /// # Errors
    ///
    /// `tui::terminal_io` when the descriptor cannot be duplicated.
    pub fn reader(&self) -> Result<File, TuiError> {
        self.tty.try_clone().map_err(TuiError::terminal_io)
    }

    /// A handle for the ratatui backend.
    ///
    /// # Errors
    ///
    /// `tui::terminal_io` when the descriptor cannot be duplicated.
    pub fn writer(&self) -> Result<File, TuiError> {
        self.tty.try_clone().map_err(TuiError::terminal_io)
    }

    /// Undoes every mode, newest first.
    pub fn restore(&mut self) {
        modes::restore();
    }
}

impl Drop for Term {
    fn drop(&mut self) {
        modes::restore();
    }
}
```

`Term` is not unit-tested (it needs a controlling terminal); the PTY tests in Task 11 cover it.

- [ ] **Step 4: Gates; commit** `feat: terminal setup and probe reader`.

### Task 8: the input thread

**Files:**

- Create: `crates/pawsman-tui/src/input/reader.rs`, `crates/pawsman-tui/src/app.rs` (only `Msg` for now)
- Modify: `input.rs`, `lib.rs`

**Interfaces:**

- Consumes: `Decoder`, `ESC_TIMEOUT`, `probe::poll_in`-style polling.
- Produces:
  - `app::Msg::{Input(Input), Resize, Terminate, InputClosed}` (`pub(crate)`).
  - `input::Reader::spawn(tty: File, tx: Sender<Msg>) -> io::Result<Reader>`: registers SIGWINCH and SIGTERM/SIGHUP/SIGINT on signal pipes, starts the thread `pawsman-input`; `Reader::stop(self)` wakes and joins it and unregisters the signals.
  - `input::reader::run(tty: impl Read + AsFd, winch: UnixStream, term: UnixStream, stop: UnixStream, tx: Sender<Msg>)` (`pub(crate)`, testable without signals).

- [ ] **Step 1: Failing tests** in `reader.rs` (stub `run` returns at once):

```rust
#[cfg(test)]
mod tests {
    use std::io::Write as _;
    use std::os::unix::net::UnixStream;
    use std::sync::mpsc;
    use std::time::Duration;

    use pawsman_core::keys::{Chord, KeyCode};

    use super::*;
    use crate::input::Input;

    struct Rig {
        tty: UnixStream,
        winch: UnixStream,
        term: UnixStream,
        stop: UnixStream,
        rx: mpsc::Receiver<Msg>,
        handle: std::thread::JoinHandle<()>,
    }

    fn rig() -> Rig {
        let (tty, tty_in) = UnixStream::pair().unwrap();
        let (winch, winch_in) = UnixStream::pair().unwrap();
        let (term, term_in) = UnixStream::pair().unwrap();
        let (stop, stop_in) = UnixStream::pair().unwrap();
        let (tx, rx) = mpsc::channel();
        let handle = std::thread::spawn(move || run(tty_in, winch_in, term_in, stop_in, tx));
        Rig { tty, winch, term, stop, rx, handle }
    }

    fn next(rx: &mpsc::Receiver<Msg>) -> Msg {
        rx.recv_timeout(Duration::from_secs(1)).expect("a message within a second")
    }

    #[test]
    fn keys_arrive_and_a_lone_esc_flushes_after_the_timeout() {
        let mut rig = rig();

        rig.tty.write_all(b"a\x1b").unwrap();

        assert_eq!(next(&rig.rx), Msg::Input(Input::Key(Chord::plain(KeyCode::Char('a')))), "a");
        assert_eq!(next(&rig.rx), Msg::Input(Input::Key(Chord::plain(KeyCode::Esc))), "Esc after the timeout");
        rig.stop.write_all(b"x").unwrap();
        rig.handle.join().unwrap();
    }

    #[test]
    fn signals_become_messages() {
        let mut rig = rig();

        rig.winch.write_all(b"w").unwrap();
        let resize = next(&rig.rx);
        rig.term.write_all(b"t").unwrap();
        let terminate = next(&rig.rx);

        assert_eq!(resize, Msg::Resize, "SIGWINCH");
        assert_eq!(terminate, Msg::Terminate, "SIGTERM, SIGHUP, SIGINT");
        rig.handle.join().unwrap();
    }

    #[test]
    fn a_closed_tty_reports_input_closed() {
        let rig = rig();

        drop(rig.tty);

        assert_eq!(next(&rig.rx), Msg::InputClosed, "end of input");
        rig.handle.join().unwrap();
    }

    #[test]
    fn stop_ends_the_thread_quietly() {
        let mut rig = rig();

        rig.stop.write_all(b"x").unwrap();
        rig.handle.join().unwrap();

        assert!(rig.rx.try_recv().is_err(), "no message on stop");
    }
}
```

`Msg` derives `Debug, Clone, PartialEq, Eq`.

- [ ] **Step 2: Run; see failures; commit** `test: input thread`.

- [ ] **Step 3: Implement**

`app.rs` (start):

```rust
//! The main loop and its state.

use crate::input::Input;

/// Everything the main loop reacts to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Msg {
    /// Decoded input.
    Input(Input),
    /// The terminal was resized.
    Resize,
    /// SIGTERM, SIGHUP or SIGINT.
    Terminate,
    /// The input thread hit end of file or an error.
    InputClosed,
}
```

`input/reader.rs`:

```rust
//! The input thread: polls the tty and the signal pipes, decodes bytes and sends `Msg`s.

use std::io::{self, Read};
use std::os::fd::AsFd;
use std::os::unix::net::UnixStream;
use std::sync::mpsc::Sender;
use std::thread::JoinHandle;

use rustix::event::{PollFd, PollFlags, Timespec, poll};
use signal_hook::SigId;
use signal_hook::consts::{SIGHUP, SIGINT, SIGTERM, SIGWINCH};
use tracing::{debug, warn};

use super::{Decoder, ESC_TIMEOUT};
use crate::app::Msg;

/// The running input thread.
pub(crate) struct Reader {
    handle: JoinHandle<()>,
    stop: UnixStream,
    signals: Vec<SigId>,
}

impl Reader {
    /// Starts the thread on `tty`, sending to `tx`.
    pub(crate) fn spawn(tty: std::fs::File, tx: Sender<Msg>) -> io::Result<Self> {
        let (winch_w, winch_r) = UnixStream::pair()?;
        let (term_w, term_r) = UnixStream::pair()?;
        let (stop, stop_r) = UnixStream::pair()?;
        let mut signals = vec![signal_hook::low_level::pipe::register(SIGWINCH, winch_w)?];
        for signal in [SIGTERM, SIGHUP, SIGINT] {
            signals.push(signal_hook::low_level::pipe::register(signal, term_w.try_clone()?)?);
        }
        let handle = std::thread::Builder::new()
            .name("pawsman-input".into())
            .spawn(move || run(tty, winch_r, term_r, stop_r, tx))?;
        Ok(Self { handle, stop, signals })
    }

    /// Stops and joins the thread and unregisters the signal handlers.
    pub(crate) fn stop(mut self) {
        use std::io::Write as _;
        let _ = self.stop.write_all(b"x");
        let _ = self.handle.join();
        for id in self.signals {
            signal_hook::low_level::unregister(id);
        }
    }
}

/// The thread body.
pub(crate) fn run(mut tty: impl Read + AsFd, mut winch: UnixStream, mut term: UnixStream, stop: UnixStream, tx: Sender<Msg>) {
    let mut decoder = Decoder::new();
    let mut buf = [0u8; 4096];
    loop {
        let timeout = decoder.has_pending().then(|| Timespec { tv_sec: 0, tv_nsec: i64::from(ESC_TIMEOUT.subsec_nanos()) });
        let mut fds = [
            PollFd::new(&tty, PollFlags::IN),
            PollFd::new(&winch, PollFlags::IN),
            PollFd::new(&term, PollFlags::IN),
            PollFd::new(&stop, PollFlags::IN),
        ];
        match poll(&mut fds, timeout.as_ref()) {
            Ok(0) => {
                for input in decoder.flush() {
                    if tx.send(Msg::Input(input)).is_err() {
                        return;
                    }
                }
                continue;
            }
            Ok(_) => {}
            Err(rustix::io::Errno::INTR) => continue,
            Err(err) => {
                warn!(%err, "input poll");
                let _ = tx.send(Msg::InputClosed);
                return;
            }
        }
        let ready = |i: usize| fds.get(i).is_some_and(|f| !f.revents().is_empty());
        let (tty_ready, winch_ready, term_ready, stop_ready) = (ready(0), ready(1), ready(2), ready(3));
        if stop_ready {
            debug!("input thread stopped");
            return;
        }
        if term_ready {
            drain(&mut term);
            let _ = tx.send(Msg::Terminate);
            return;
        }
        if winch_ready {
            drain(&mut winch);
            if tx.send(Msg::Resize).is_err() {
                return;
            }
        }
        if tty_ready {
            match tty.read(&mut buf) {
                Ok(0) | Err(_) => {
                    let _ = tx.send(Msg::InputClosed);
                    return;
                }
                Ok(n) => {
                    for input in decoder.feed(buf.get(..n).unwrap_or_default()) {
                        if tx.send(Msg::Input(input)).is_err() {
                            return;
                        }
                    }
                }
            }
        }
    }
}

fn drain(pipe: &mut UnixStream) {
    let mut sink = [0u8; 64];
    let _ = pipe.read(&mut sink);
}
```

Re-export `pub(crate) use reader::Reader;` from `input.rs` (`mod reader;`); `lib.rs` adds `mod app;`.

- [ ] **Step 4: Gates; commit** `feat: input thread with signal pipes`.

### Task 9: colours and the splash

**Files:**

- Create: `crates/pawsman-tui/src/views.rs`, `views/colors.rs`, `views/splash.rs`

**Interfaces:**

- Consumes: `ColorDepth`.
- Produces:
  - `colors::downsample(buf: &mut Buffer, area: Rect, depth: ColorDepth)`, `colors::to_indexed(Color) -> Color`, `colors::to_ansi16(Color) -> Color`.
  - `splash::Splash::new(seed: u32, depth: ColorDepth)`, `frame(&mut self, elapsed: Duration, buf: &mut Buffer, area: Rect)`, `skip(&mut self)`, `done(&self, elapsed: Duration) -> bool`, `next_tick(&self, elapsed: Duration) -> Option<Duration>`; `splash::fits(Rect) -> bool`; `splash::draw_logo(buf: &mut Buffer, x: u16, y: u16)` (final colours, no background); `splash::{LOGO, LOGO_W, LOGO_H, TAGLINE, BG}`.

- [ ] **Step 1: Failing tests** (`splash.rs`): the six PoC tests (deterministic frame with a seed, done after 1.3 s, skip finishes, a too-small area draws nothing and does not panic, `fits` false below the logo, indexed leaves no RGB) plus:

```rust
    #[test]
    fn ansi16_shows_the_final_frame_once_and_waits() {
        let area = Rect::new(0, 0, 80, 24);
        let mut buf = Buffer::empty(area);
        let mut splash = Splash::new(42, ColorDepth::Ansi16);

        splash.frame(Duration::ZERO, &mut buf, area);

        let indexed = buf.content.iter().all(|c| !matches!(c.fg, Color::Rgb(..) | Color::Indexed(_)));
        assert!(indexed, "only the 16 named colours");
        assert_eq!(splash.next_tick(Duration::ZERO), Some(STATIC_FOR), "one timer to the end, no frames");
        assert!(splash.done(STATIC_FOR), "done after the static time");
    }

    #[test]
    fn animated_ticks_every_16ms_until_done() {
        let mut splash = Splash::new(42, ColorDepth::TrueColor);

        assert_eq!(splash.next_tick(Duration::ZERO), Some(Duration::from_millis(16)), "ticking");
        splash.skip();
        assert_eq!(splash.next_tick(Duration::from_millis(20)), None, "no timer once done");
    }

    #[test]
    fn effects_stay_inside_the_logo() {
        let area = Rect::new(0, 0, 120, 40);
        let mut buf = Buffer::empty(area);
        let mut splash = Splash::new(42, ColorDepth::TrueColor);

        splash.frame(Duration::from_millis(600), &mut buf, area);

        let corner = &buf[(0, 0)];
        assert_eq!(corner.bg, BG, "the corner keeps the plain background");
        assert_eq!(corner.symbol(), " ", "and no glyph");
    }

    #[test]
    fn tagline_is_centred_by_width() {
        let area = Rect::new(0, 0, 80, 24);
        let mut buf = Buffer::empty(area);
        let mut splash = Splash::new(42, ColorDepth::TrueColor);
        splash.skip();

        splash.frame(Duration::from_millis(2000), &mut buf, area);

        let row: String = (0..80).map(|x| buf[(x, 13)].symbol().to_owned()).collect();
        let start = row.find(TAGLINE).expect("tagline on row 13");
        assert_eq!(start, (80 - TAGLINE.len()) / 2, "centred: {row:?}");
    }
```

Snapshot the final frame text with insta: `insta::assert_snapshot!(buffer_text(&buf))` where `buffer_text` joins rows of symbols; review the `.snap.new` and accept.

`colors.rs` tests:

```rust
#[cfg(test)]
mod tests {
    use ratatui::style::Color;
    use rstest::rstest;

    use super::*;

    #[rstest]
    #[case(Color::Rgb(0, 0, 0), Color::Indexed(16))]
    #[case(Color::Rgb(255, 255, 255), Color::Indexed(231))]
    #[case(Color::Rgb(128, 128, 128), Color::Indexed(244))]
    #[case(Color::Red, Color::Red)]
    fn indexed_nearest(#[case] from: Color, #[case] expected: Color) {
        assert_eq!(to_indexed(from), expected, "{from:?}");
    }

    #[rstest]
    #[case(Color::Rgb(255, 105, 180), Color::LightMagenta)]
    #[case(Color::Rgb(90, 210, 255), Color::LightCyan)]
    #[case(Color::Rgb(22, 22, 34), Color::Black)]
    #[case(Color::Indexed(196), Color::LightRed)]
    fn ansi16_nearest(#[case] from: Color, #[case] expected: Color) {
        assert_eq!(to_ansi16(from), expected, "{from:?}");
    }
}
```

- [ ] **Step 2: Run; see failures; commit** `test: splash and colour downsampling`.

- [ ] **Step 3: Implement** by porting `pawsman-poc/splash/src/splash.rs` with these changes:
  - `Splash::new(seed, depth)`: `ColorDepth::Ansi16` builds a static splash (`mode: Mode::Static`), others the PoC effect; `Indexed256` downsamples after each frame.
  - `const STATIC_FOR: Duration = Duration::from_millis(1200)`; `done(elapsed)` is `effect.done()` when animated, `elapsed >= STATIC_FOR` when static; `next_tick(elapsed)` is `Some(TICK)` while animating, `Some(STATIC_FOR - elapsed)` while static, `None` when done.
  - `frame` returns early when `!fits(area)`; paints, advances (animated only) and then `colors::downsample(buf, area, depth)`.
  - The tagline is centred with `unicode_width::UnicodeWidthStr::width(TAGLINE)`, not `len()`.
  - `draw_logo(buf, x, y)` paints the gradient rows without touching the background; `paint` calls it after filling `BG`.
  - Casts use `u16::try_from(...).unwrap_or(u16::MAX)` and `f32::from` so clippy pedantic passes; indexing goes through `buf.cell_mut`.

`colors.rs`: `to_indexed` from the PoC; `to_ansi16` picks the nearest of the 16 colours by squared RGB distance using the xterm defaults (black 0,0,0; red 205,0,0; green 0,205,0; yellow 205,205,0; blue 0,0,238; magenta 205,0,205; cyan 0,205,205; white 229,229,229; bright black 127,127,127; bright red 255,0,0; bright green 0,255,0; bright yellow 255,255,0; bright blue 92,92,255; bright magenta 255,0,255; bright cyan 0,255,255; bright white 255,255,255), mapping `Indexed(n)` through the xterm 256 palette first; named colours pass through. `downsample` maps `fg` and `bg` of every cell in `area` for `Indexed256` and `Ansi16`, and does nothing for `TrueColor`.

`views.rs`: `pub(crate) mod colors; pub(crate) mod splash;`; `lib.rs` adds `mod views;`.

- [ ] **Step 4: Gates; accept the snapshot after reading it; commit** `feat: startup splash with colour downsampling`.

### Task 10: the app state machine and the home, help and hint views

**Files:**

- Create: `views/home.rs`, `views/help.rs`, `views/hints.rs`
- Modify: `app.rs`, `views.rs`

**Interfaces:**

- Consumes: `Msg`, `Splash`, `keys::{lookup, hints, table, Action, Context}`, `Notice`, `Caps`.
- Produces:
  - `app::SplashSetting::{On, Off}` (pub, re-exported).
  - `app::App::new(splash: SplashSetting, caps: Caps, notices: Vec<Notice>) -> App` (`pub(crate)`), `App::update(&mut self, Msg, now: Duration) -> Result<Flow, TuiError>`, `App::draw(&mut self, frame: &mut Frame, now: Duration)`, `App::next_tick(&self, now: Duration) -> Option<Duration>`; `Flow::{Continue, Quit}`.
  - `views::home::draw(frame, area, notice: Option<&Notice>, hint: Option<CapsHint>)`, `views::help::draw(frame, area)`, `views::hints::line(Context) -> Line<'static>`.
  - `NOTICE_FOR: Duration` (5 s).

- [ ] **Step 1: Failing tests** in `app.rs`:

```rust
#[cfg(test)]
mod tests {
    use std::time::Duration;

    use pawsman_core::env::EnvSnapshot;
    use pawsman_core::keys::{Chord, KeyCode};
    use pawsman_core::notice::Notice;
    use pawsman_core::settings::ImageMode;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    use super::*;
    use crate::input::Input;
    use crate::term::Caps;
    use crate::term::probe::parse;

    fn caps(colorterm: &str) -> Caps {
        let env = EnvSnapshot::from_pairs([("COLORTERM", colorterm), ("TERM", "xterm-256color"), ("LANG", "C.UTF-8")]);
        Caps::resolve(&parse(b""), &env, ImageMode::Auto)
    }

    fn key(chord: Chord) -> Msg {
        Msg::Input(Input::Key(chord))
    }

    fn render(app: &mut App, w: u16, h: u16, now: Duration) -> Terminal<TestBackend> {
        let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
        terminal.draw(|f| app.draw(f, now)).unwrap();
        terminal
    }

    fn screen(terminal: &Terminal<TestBackend>) -> String {
        terminal.backend().to_string()
    }

    #[test]
    fn ctrl_q_quits_from_the_splash() {
        let mut app = App::new(SplashSetting::On, caps("truecolor"), vec![]);

        let flow = app.update(key(Chord::ctrl('q')), Duration::from_millis(100)).unwrap();

        assert_eq!(flow, Flow::Quit, "quit during the splash");
    }

    #[test]
    fn any_other_key_skips_to_home() {
        let mut app = App::new(SplashSetting::On, caps("truecolor"), vec![]);

        app.update(key(Chord::plain(KeyCode::Char('x'))), Duration::from_millis(100)).unwrap();
        let terminal = render(&mut app, 80, 24, Duration::from_millis(116));

        assert!(screen(&terminal).contains("no collection open"), "home after a key");
        assert_eq!(app.next_tick(Duration::from_millis(116)), None, "no timer on a still home");
    }

    #[test]
    fn splash_ends_by_itself() {
        let mut app = App::new(SplashSetting::On, caps("truecolor"), vec![]);
        let mut now = Duration::ZERO;
        while let Some(tick) = app.next_tick(now) {
            render(&mut app, 80, 24, now);
            now += tick;
            assert!(now < Duration::from_secs(3), "splash never ended");
        }

        let terminal = render(&mut app, 80, 24, now);

        assert!(screen(&terminal).contains("no collection open"), "home after the splash");
    }

    #[test]
    fn splash_off_starts_on_home() {
        let mut app = App::new(SplashSetting::Off, caps("truecolor"), vec![]);

        let terminal = render(&mut app, 80, 24, Duration::ZERO);

        assert!(screen(&terminal).contains("no collection open"), "home at once");
    }

    #[test]
    fn f1_opens_help_and_esc_closes_it() {
        let mut app = App::new(SplashSetting::Off, caps("truecolor"), vec![]);

        app.update(key(Chord::plain(KeyCode::F(1))), Duration::ZERO).unwrap();
        let open = screen(&render(&mut app, 80, 24, Duration::ZERO));
        app.update(key(Chord::plain(KeyCode::Esc)), Duration::ZERO).unwrap();
        let closed = screen(&render(&mut app, 80, 24, Duration::ZERO));

        assert!(open.contains("Ctrl-Q") && open.contains("copy the selection"), "help lists the table:\n{open}");
        assert!(!closed.contains("copy the selection"), "closed:\n{closed}");
    }

    #[test]
    fn ctrl_c_with_nothing_selected_names_ctrl_q() {
        let mut app = App::new(SplashSetting::Off, caps("truecolor"), vec![]);

        app.update(key(Chord::ctrl('c')), Duration::ZERO).unwrap();
        let text = screen(&render(&mut app, 80, 24, Duration::ZERO));

        assert!(text.contains("nothing selected") && text.contains("Ctrl-Q quits"), "hint:\n{text}");
        assert_eq!(app.next_tick(Duration::ZERO), Some(NOTICE_FOR), "a timer for the notice to expire");
    }

    #[test]
    fn notices_expire() {
        let mut app = App::new(SplashSetting::Off, caps("truecolor"), vec![Notice::warn("no log file")]);
        render(&mut app, 80, 24, Duration::ZERO);

        let later = screen(&render(&mut app, 80, 24, NOTICE_FOR + Duration::from_millis(1)));

        assert!(!later.contains("no log file"), "gone after {NOTICE_FOR:?}");
    }

    #[test]
    fn terminate_quits_and_input_closed_is_an_error() {
        let mut app = App::new(SplashSetting::Off, caps("truecolor"), vec![]);

        let flow = app.update(Msg::Terminate, Duration::ZERO).unwrap();
        let err = app.update(Msg::InputClosed, Duration::ZERO).unwrap_err();

        assert_eq!(flow, Flow::Quit, "signal quits");
        assert_eq!(pawsman_error::ErrorInfo::code(&err), crate::codes::INPUT_CLOSED, "closed input");
    }

    #[test]
    fn tiny_areas_never_panic() {
        for (w, h) in [(0, 0), (1, 1), (10, 3), (44, 5)] {
            let mut app = App::new(SplashSetting::On, caps("truecolor"), vec![Notice::info("x")]);
            render(&mut app, w, h, Duration::from_millis(300));
            app.update(key(Chord::plain(KeyCode::F(1))), Duration::from_millis(300)).unwrap();
            render(&mut app, w, h, Duration::from_millis(400));
        }
    }

    #[test]
    fn a_too_small_terminal_ends_the_splash() {
        let mut app = App::new(SplashSetting::On, caps("truecolor"), vec![]);

        render(&mut app, 20, 5, Duration::from_millis(100));

        assert_eq!(app.next_tick(Duration::from_millis(100)), None, "splash over below the logo size");
    }

    #[test]
    fn home_snapshot_80x24() {
        let mut app = App::new(SplashSetting::Off, caps("truecolor"), vec![]);

        let terminal = render(&mut app, 80, 24, Duration::ZERO);

        insta::assert_snapshot!(terminal.backend());
    }

    #[test]
    fn help_snapshot_80x24() {
        let mut app = App::new(SplashSetting::Off, caps("truecolor"), vec![]);
        app.update(key(Chord::plain(KeyCode::F(1))), Duration::ZERO).unwrap();

        let terminal = render(&mut app, 80, 24, Duration::ZERO);

        insta::assert_snapshot!(terminal.backend());
    }

    #[test]
    fn caps_hint_shows_on_home() {
        let env = EnvSnapshot::from_pairs([("TMUX", "/t,1,0"), ("LANG", "C")]);
        let caps = Caps::resolve(&parse(b""), &env, ImageMode::Auto);
        let mut app = App::new(SplashSetting::Off, caps, vec![]);

        let text = screen(&render(&mut app, 100, 24, Duration::ZERO));

        assert!(text.contains("tmux with -u"), "UTF-8 hint:\n{text}");
    }

    #[test]
    fn mouse_paste_and_focus_change_nothing() {
        let mut app = App::new(SplashSetting::Off, caps("truecolor"), vec![]);
        let before = screen(&render(&mut app, 80, 24, Duration::ZERO));

        for msg in [Msg::Input(Input::Paste("x".into())), Msg::Input(Input::FocusLost), Msg::Resize] {
            assert_eq!(app.update(msg, Duration::ZERO).unwrap(), Flow::Continue, "continues");
        }
        let after = screen(&render(&mut app, 80, 24, Duration::ZERO));

        assert_eq!(before, after, "same screen");
    }
}
```

- [ ] **Step 2: Run; see failures; commit** `test: app state machine and home views`.

- [ ] **Step 3: Implement**

`app.rs` additions:

```rust
/// Whether the startup splash plays.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SplashSetting {
    /// Play it.
    On,
    /// Start on the main view.
    Off,
}

/// What the loop does after a message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Flow {
    Continue,
    Quit,
}

/// How long a notice stays on screen.
pub(crate) const NOTICE_FOR: Duration = Duration::from_secs(5);

enum Screen {
    Splash(Splash),
    Home,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Overlay {
    None,
    Help,
}

pub(crate) struct App {
    screen: Screen,
    overlay: Overlay,
    caps: Caps,
    notices: VecDeque<Notice>,
    /// The current notice and when it expires, set when it is first drawn.
    shown: Option<(Notice, Duration)>,
}

impl App {
    pub(crate) fn new(splash: SplashSetting, caps: Caps, notices: Vec<Notice>) -> Self {
        let screen = match splash {
            SplashSetting::On => Screen::Splash(Splash::new(splash_seed(), caps.color())),
            SplashSetting::Off => Screen::Home,
        };
        Self { screen, overlay: Overlay::None, caps, notices: notices.into(), shown: None }
    }

    fn context(&self) -> Context {
        match (&self.screen, self.overlay) {
            (Screen::Splash(_), _) => Context::Splash,
            (Screen::Home, Overlay::Help) => Context::Help,
            (Screen::Home, Overlay::None) => Context::Home,
        }
    }

    pub(crate) fn update(&mut self, msg: Msg, _now: Duration) -> Result<Flow, TuiError> {
        let chord = match msg {
            Msg::Terminate => return Ok(Flow::Quit),
            Msg::InputClosed => return Err(TuiError::input_closed()),
            Msg::Resize | Msg::Input(Input::Paste(_) | Input::Mouse(_) | Input::FocusGained | Input::FocusLost) => {
                return Ok(Flow::Continue);
            }
            Msg::Input(Input::Key(chord)) => chord,
        };
        let action = keys::lookup(self.context(), chord);
        if let Screen::Splash(_) = self.screen
            && action != Some(Action::Quit)
        {
            self.screen = Screen::Home;
            return Ok(Flow::Continue);
        }
        match action {
            Some(Action::Quit) => return Ok(Flow::Quit),
            Some(Action::Help) => self.overlay = Overlay::Help,
            Some(Action::Close) => self.overlay = Overlay::None,
            Some(Action::Copy) => self.notices.push_back(Notice::info(format!("nothing selected · {} quits", quit_label()))),
            Some(_) => {}
            None if !chord.mods().is_empty() => tracing::debug!(%chord, "unbound key"),
            None => {}
        }
        Ok(Flow::Continue)
    }

    pub(crate) fn draw(&mut self, frame: &mut Frame, now: Duration) {
        let area = frame.area();
        if let Screen::Splash(splash) = &mut self.screen {
            if splash.done(now) || !splash::fits(area) {
                self.screen = Screen::Home;
            } else {
                splash.frame(now, frame.buffer_mut(), area);
                return;
            }
        }
        self.expire(now);
        home::draw(frame, area, self.shown.as_ref().map(|(n, _)| n), self.caps.hint());
        if self.overlay == Overlay::Help {
            help::draw(frame, area);
        }
    }

    fn expire(&mut self, now: Duration) {
        if self.shown.as_ref().is_some_and(|(_, until)| now >= *until) {
            self.shown = None;
        }
        if self.shown.is_none()
            && let Some(next) = self.notices.pop_front()
        {
            self.shown = Some((next, now + NOTICE_FOR));
        }
    }

    pub(crate) fn next_tick(&self, now: Duration) -> Option<Duration> {
        match &self.screen {
            Screen::Splash(splash) => splash.next_tick(now).or(Some(Duration::ZERO)),
            Screen::Home => match (&self.shown, self.notices.is_empty()) {
                (Some((_, until)), _) => Some(until.saturating_sub(now)),
                (None, false) => Some(Duration::ZERO),
                (None, true) => None,
            },
        }
    }
}

fn quit_label() -> String {
    keys::table()
        .iter()
        .find(|b| b.action() == Action::Quit)
        .and_then(|b| b.chords().first())
        .map_or_else(|| "Ctrl-Q".to_owned(), ToString::to_string)
}

/// The splash's coalesce pattern changes from run to run.
fn splash_seed() -> u32 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(42, |d| d.subsec_nanos())
}
```

Notes for the implementer: `Screen::Splash` → `next_tick` returning `Some(Duration::ZERO)` when the splash is done but not yet replaced makes the loop draw once more, which switches to home. `ctrl_c_with_nothing_selected_names_ctrl_q` expects `Some(NOTICE_FOR)` right after the draw that shows the notice. Tests that need deterministic splash frames construct `Splash::new(42, …)` directly, not through `App`.

`views/hints.rs`:

```rust
//! The hint bar, generated from the key table.

use pawsman_core::keys::{Context, hints};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

/// `Ctrl-Q quit · F1 help` for `context`.
pub(crate) fn line(context: Context) -> Line<'static> {
    let mut spans = Vec::new();
    for (i, (key, hint)) in hints(context).into_iter().enumerate() {
        if i > 0 {
            spans.push(Span::raw(" · "));
        }
        spans.push(Span::styled(key, Style::new().add_modifier(Modifier::BOLD)));
        spans.push(Span::raw(format!(" {hint}")));
    }
    Line::from(spans)
}
```

`views/home.rs`: background fill; if the area fits the logo plus three rows, `splash::draw_logo` centred above the middle, else the word `pawsman` centred; `no collection open` two rows below; the notice (styled by `Level`: Info dim, Warn yellow, Error red) on the second-to-last row; the hint bar (`hints::line(Context::Home)`, plus `· <caps hint>` when there is one) on the last row. Every coordinate is clamped with `saturating_sub` and drawn through `Paragraph` into `Rect`s intersected with `area`, so 0×0 and 1×1 draw nothing and never panic.

`views/help.rs`: a centred `Block::bordered().title(" keys ")` box, `Clear` underneath, one row per `keys::table()` binding: chord labels joined by ` / `, padded to the widest label measured with `unicode-width`, then the hint; the hint bar for `Context::Help` at the bottom of the box. The box shrinks to the area and draws nothing below 3×3.

- [ ] **Step 4: Gates; read and accept the two snapshots; commit** `feat: app state machine with home, help and hint views`.

### Task 11: `run`, the `pawsman` binary and PTY tests

**Files:**

- Modify: `app.rs` (event loop), `lib.rs` (`run`, `Options`), `Cargo.toml` (workspace member deps)
- Create: `crates/pawsman-cli/Cargo.toml`, `src/main.rs`, `src/args.rs`, `src/log.rs`, `tests/it/main.rs`, `tests/it/pty.rs`, `tests/it/probe_paths.rs`, `tests/it/lifecycle.rs`

**Interfaces:**

- Consumes: everything above.
- Produces:
  - `pawsman_tui::Options::new(env: EnvSnapshot, splash: SplashSetting, images: ImageMode)`, `.notices(Vec<Notice>)`; `pawsman_tui::run(Options) -> Result<(), TuiError>`.
  - The binary `pawsman`: `--no-splash`, `--images <MODE>`, `--version`, `--help`.
  - PTY harness `pty::Session::spawn(&Spec) -> Session`, `Session::{wait_for_output(&[u8]), wait_for_screen(&str), write(&[u8]), wait_exit(Duration) -> ExitStatus, raw() -> Vec<u8>, log() -> String, kill(Signal)}`.

- [ ] **Step 1: Failing PTY tests** (`tests/it/probe_paths.rs`, `tests/it/lifecycle.rs`). The harness spawns `env!("CARGO_BIN_EXE_pawsman")` with `env_clear()` plus `HOME=<tempdir>`, `TERM`, `LANG`, `PAWSMAN_LOG=pawsman=debug` and the case's variables, in an 80×24 PTY from `pty_process::blocking`, reads the PTY on a thread into a shared buffer fed into a `vt100::Parser`, waits for the probe query (`\x1b[c` in the output), then writes the canned replies.

```rust
// tests/it/probe_paths.rs
use std::time::Duration;

use crate::pty::{Session, Spec};

const KITTY: &[u8] = b"\x1b_Gi=31;OK\x1b\\\x1b[?1u\x1b[6;20;10t\x1bP1$r48:2:1:2:3m\x1b\\\x1b[?62;22c";

#[test]
fn kitty_gets_kitty_graphics_and_keyboard_and_quits_on_kitty_ctrl_q() {
    let mut s = Session::spawn(&Spec::new().arg("--no-splash").env("TERM", "xterm-kitty").replies(KITTY));

    s.wait_for_screen("no collection open");
    s.write(b"\x1b[113;5u");
    let status = s.wait_exit(Duration::from_secs(3));

    assert!(status.success(), "clean exit: {status:?}");
    let log = s.log();
    assert!(log.contains("graphics=Kitty") && log.contains("keyboard=Kitty"), "probe result in the log:\n{log}");
    assert!(crate::pty::contains(&s.raw(), b"\x1b[>1u"), "kitty keyboard flags pushed");
}

#[test]
fn kitty_cyrillic_ctrl_q_quits() {
    let mut s = Session::spawn(&Spec::new().arg("--no-splash").env("TERM", "xterm-kitty").replies(KITTY));

    s.wait_for_screen("no collection open");
    s.write(b"\x1b[1081;5u");

    assert!(s.wait_exit(Duration::from_secs(3)).success(), "Ctrl-й quits");
}

#[test]
fn tmux_with_passthrough_uses_the_outer_da1() {
    let replies = b"\x1b[?1;2;4c\x1b_Gi=31;OK\x1b\\\x1b[?62;22c";
    let mut s = Session::spawn(
        &Spec::new().arg("--no-splash").env("TERM", "tmux-256color").env("TMUX", "/tmp/t,1,0").replies(replies),
    );

    s.wait_for_screen("no collection open");
    s.write(b"\x11");
    s.wait_exit(Duration::from_secs(3));

    let log = s.log();
    assert!(log.contains("graphics=Kitty") && log.contains("keyboard=Legacy"), "outer terminal decides:\n{log}");
    assert!(crate::pty::contains(&s.raw(), b"\x1bPtmux;\x1b\x1b[c\x1b\\"), "second DA1 through passthrough");
}

#[test]
fn tmux_without_passthrough_ends_the_probe_after_the_tmux_grace() {
    let mut s = Session::spawn(
        &Spec::new().arg("--no-splash").env("TERM", "tmux-256color").env("TMUX", "/tmp/t,1,0").replies(b"\x1b[?1;2;4c"),
    );

    s.wait_for_screen("allow-passthrough on");
    s.write(b"\x11");
    s.wait_exit(Duration::from_secs(3));

    let log = s.log();
    assert!(log.contains("graphics=Blocks"), "tmux's sixel claim ignored:\n{log}");
    let ms = crate::pty::probe_ms(&log);
    assert!((450..1000).contains(&ms), "probe took {ms} ms");
}

#[test]
fn tmux_without_utf8_turns_images_off_with_a_hint() {
    let replies = b"\x1b[?1;2;4c\x1b_Gi=31;OK\x1b\\\x1b[?62;22c";
    let mut s = Session::spawn(
        &Spec::new().arg("--no-splash").env("LANG", "C").env("TERM", "tmux-256color").env("TMUX", "/tmp/t,1,0").replies(replies),
    );

    s.wait_for_screen("tmux with -u");
    s.write(b"\x11");
    s.wait_exit(Duration::from_secs(3));

    assert!(s.log().contains("graphics=Off"), "off without UTF-8");
}

#[test]
fn xterm_gets_sixel_and_legacy_ctrl_q_quits() {
    let mut s = Session::spawn(&Spec::new().arg("--no-splash").env("TERM", "xterm").replies(b"\x1b[?63;1;2;4;6;9;15;22c"));

    s.wait_for_screen("no collection open");
    s.write(b"\x11");

    assert!(s.wait_exit(Duration::from_secs(3)).success(), "legacy Ctrl-Q quits");
    assert!(s.log().contains("graphics=Sixel"), "sixel from DA1");
}

#[test]
fn silence_times_out_and_a_late_reply_types_nothing() {
    let mut s = Session::spawn(&Spec::new().arg("--no-splash").env("TERM", "xterm-256color"));

    s.wait_for_screen("no collection open");
    s.write(b"\x1b_Gi=31;OK\x1b\\\x1b[?62;22c");
    std::thread::sleep(Duration::from_millis(200));
    s.write(b"\x11");
    s.wait_exit(Duration::from_secs(3));

    let log = s.log();
    let ms = crate::pty::probe_ms(&log);
    assert!((1100..1600).contains(&ms), "probe waited 1 s + grace: {ms} ms");
    assert!(log.contains("dropped a terminal reply"), "late reply dropped:\n{log}");
    assert!(!log.contains("unbound key"), "nothing typed by the reply:\n{log}");
}
```

```rust
// tests/it/lifecycle.rs
use std::time::Duration;

use crate::pty::{Session, Spec};

#[test]
fn ctrl_q_restores_every_mode_newest_first() {
    let mut s = Session::spawn(&Spec::new().arg("--no-splash").env("TERM", "xterm-kitty").replies(super::probe_paths::KITTY));

    s.wait_for_screen("no collection open");
    s.write(b"\x11");
    s.wait_exit(Duration::from_secs(3));

    let raw = s.raw();
    let tail = b"\x1b[?1006l\x1b[?1002l\x1b[?1000l\x1b[?1004l\x1b[?2004l\x1b[<u\x1b[?25h\x1b[?1049l";
    assert!(crate::pty::contains(&raw, tail), "undo in order");
}

#[test]
fn the_splash_plays_then_home_shows() {
    let mut s = Session::spawn(&Spec::new().env("TERM", "xterm-256color").env("COLORTERM", "truecolor"));

    s.wait_for_screen("a terminal API client");
    s.wait_for_screen("no collection open");
    s.write(b"\x11");

    assert!(s.wait_exit(Duration::from_secs(3)).success(), "clean exit");
}

#[test]
fn sighup_restores_and_exits() {
    let mut s = Session::spawn(&Spec::new().env("TERM", "xterm-256color"));

    s.wait_for_screen("a terminal API client");
    s.kill(rustix::process::Signal::HUP);
    let status = s.wait_exit(Duration::from_secs(1));

    assert!(status.code().is_some(), "exited by itself, not killed: {status:?}");
    assert!(crate::pty::contains(&s.raw(), b"\x1b[?1049l"), "alternate screen left");
}

#[test]
fn redirected_stdout_stays_empty() {
    let out = tempfile::NamedTempFile::new().unwrap();
    let mut s = Session::spawn(&Spec::new().arg("--no-splash").env("TERM", "xterm-256color").stdout(out.path()));

    s.wait_for_screen("no collection open");
    s.write(b"\x11");
    s.wait_exit(Duration::from_secs(3));

    assert_eq!(std::fs::metadata(out.path()).unwrap().len(), 0, "frames went to /dev/tty");
}

#[test]
fn invalid_settings_show_a_notice_and_defaults_apply() {
    let spec = Spec::new().arg("--no-splash").env("TERM", "xterm-256color").settings("[ui]\nanimation = false\n");
    let mut s = Session::spawn(&spec);

    s.wait_for_screen("core::settings_invalid");
    s.write(b"\x11");

    assert!(s.wait_exit(Duration::from_secs(3)).success(), "still runs");
}

#[test]
fn version_prints_without_a_terminal() {
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_pawsman")).arg("--version").output().unwrap();

    assert!(String::from_utf8_lossy(&out.stdout).starts_with("pawsman "), "version line");
}
```

`tests/it/main.rs`: `mod lifecycle; mod probe_paths; mod pty;` with `probe_paths::KITTY` made `pub(crate)`. Lint relaxations for test code come from `clippy.toml`; `std::process::Command::new` in `version_prints_without_a_terminal` takes `#[expect(clippy::disallowed_methods, reason = "the test runs the binary")]`.

`pty.rs` essentials:

```rust
//! A PTY session around the `pawsman` binary, with canned probe replies and a vt100 screen.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub(crate) struct Spec { args: Vec<String>, env: Vec<(String, String)>, replies: Vec<u8>, stdout: Option<PathBuf>, settings: Option<String> }

pub(crate) struct Session {
    pty: pty_process::blocking::Pty,
    child: std::process::Child,
    output: Arc<Mutex<Vec<u8>>>,
    home: tempfile::TempDir,
}

pub(crate) fn contains(hay: &[u8], needle: &[u8]) -> bool {
    hay.windows(needle.len()).any(|w| w == needle)
}

/// The `probe_ms=<n>` field of the probe log line.
pub(crate) fn probe_ms(log: &str) -> u64 {
    log.split("probe_ms=").nth(1).and_then(|r| r.split(|c: char| !c.is_ascii_digit()).next()).and_then(|n| n.parse().ok()).expect("probe_ms in the log")
}
```

`Session::spawn`: create `home`, write `settings` to `home/.config/pawsman/settings.toml` when given; `let (pty, pts) = pty_process::blocking::open()?; pty.resize(pty_process::Size::new(24, 80))?;` build `pty_process::blocking::Command::new(bin)` with `env_clear`, `HOME`, `LANG=C.UTF-8` (overridable), the spec's args and env, `stdout` redirected to the file when given, then `spawn(pts)`. A reader thread copies the PTY into `output`. If `replies` is non-empty, `spawn` waits for `\x1b[c` in `output` (2 s) and writes them. `wait_for_screen(text)` feeds `output` into a fresh `vt100::Parser::new(24, 80, 0)` every 20 ms until `parser.screen().contents()` contains `text` (5 s, then panics with the screen). `wait_exit(limit)` polls `child.try_wait()` every 10 ms, kills and panics past the limit. `log()` reads every file in `home/.local/state/pawsman/`. `kill(signal)` uses `rustix::process::kill_process(Pid::from_child(&child), signal)`. Check the exact `pty-process` 0.5.4 blocking API in its docs and adapt these calls; the behaviour above is the contract.

- [ ] **Step 2: Run; see failures** (binary missing: write `main.rs` as `fn main() {}` first so the tests compile and fail on their assertions). Commit `test: PTY tests for probe paths and lifecycle`.

- [ ] **Step 3: Implement** the loop in `app.rs`:

```rust
/// Runs the TUI until Ctrl-Q, a signal or an error; the terminal is restored on every path.
///
/// # Errors
///
/// Any `TuiError`: no terminal, a refused mode, a terminal write failure, or stopped input.
pub fn run(options: Options) -> Result<(), TuiError> {
    let mut term = Term::open()?;
    let probed = term.probe(InTmux::from_env(options.env()))?;
    let caps = Caps::resolve(&probe::parse(&probed.bytes), options.env(), options.images());
    tracing::info!(
        graphics = ?caps.graphics(),
        keyboard = ?caps.keyboard(),
        color = ?caps.color(),
        tmux = ?caps.tmux(),
        ssh = ?caps.ssh(),
        cell_px = ?caps.cell_px(),
        probe_ms = u64::try_from(probed.took.as_millis()).unwrap_or(u64::MAX),
        "probe"
    );
    term.enable_input(caps.keyboard());
    let (tx, rx) = std::sync::mpsc::channel();
    let reader = Reader::spawn(term.reader()?, tx).map_err(TuiError::terminal_io)?;
    let backend = CrosstermBackend::new(BufWriter::with_capacity(1 << 16, term.writer()?));
    let result = Terminal::new(backend).map_err(TuiError::terminal_io).and_then(|mut terminal| {
        let mut app = App::new(options.splash(), caps, options.into_notices());
        event_loop(&mut terminal, &mut app, &rx)
    });
    reader.stop();
    term.restore();
    result
}

fn event_loop<B: Backend<Error = std::io::Error>>(
    terminal: &mut Terminal<B>,
    app: &mut App,
    rx: &Receiver<Msg>,
) -> Result<(), TuiError> {
    let start = Instant::now();
    let mut frames = 0u32;
    terminal.clear().map_err(TuiError::terminal_io)?;
    loop {
        let now = start.elapsed();
        terminal.draw(|f| app.draw(f, now)).map_err(TuiError::terminal_io)?;
        frames += 1;
        let first = match app.next_tick(start.elapsed()) {
            Some(wait) => match rx.recv_timeout(wait) {
                Ok(msg) => Some(msg),
                Err(RecvTimeoutError::Timeout) => None,
                Err(RecvTimeoutError::Disconnected) => Some(Msg::InputClosed),
            },
            None => Some(rx.recv().unwrap_or(Msg::InputClosed)),
        };
        for msg in first.into_iter().chain(std::iter::from_fn(|| rx.try_recv().ok())) {
            if app.update(msg, start.elapsed())? == Flow::Quit {
                tracing::info!(frames, "quit");
                return Ok(());
            }
        }
    }
}
```

`Options` in `lib.rs` with private fields, `new(env, splash, images)`, `notices(self, Vec<Notice>) -> Self`, and `pub(crate)` getters `env()`, `splash()`, `images()`, `into_notices()`. `lib.rs`: `pub use app::{SplashSetting, run}; pub use options::Options;` (put `Options` in `src/options.rs`).

`crates/pawsman-cli/Cargo.toml`:

```toml
[package]
name = "pawsman-cli"
description = "The pawsman binary"
version.workspace = true
edition.workspace = true
rust-version.workspace = true
license.workspace = true
publish.workspace = true

[[bin]]
name = "pawsman"
path = "src/main.rs"

[dependencies]
pawsman-core.workspace = true
pawsman-error.workspace = true
pawsman-tui.workspace = true
clap.workspace = true
tracing.workspace = true
tracing-appender.workspace = true
tracing-subscriber.workspace = true

[dev-dependencies]
pty-process.workspace = true
rustix.workspace = true
tempfile.workspace = true
vt100.workspace = true

[lints]
workspace = true
```

`args.rs`:

```rust
//! Command-line flags.

use clap::Parser;
use pawsman_core::settings::ImageMode;

/// A terminal API client.
#[derive(Debug, Parser)]
#[command(name = "pawsman", version, about)]
pub(crate) struct Args {
    /// Start without the startup animation.
    #[arg(long)]
    pub(crate) no_splash: bool,
    /// How images are drawn: auto, kitty, sixel, blocks or off.
    #[arg(long, value_name = "MODE")]
    pub(crate) images: Option<ImageMode>,
}
```

`log.rs`:

```rust
//! The rolling file log; never the terminal (OBSERVABILITY P2–P4).

use std::path::Path;

use pawsman_core::env::EnvSnapshot;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_appender::rolling::{Builder, Rotation};
use tracing_subscriber::EnvFilter;

const DEFAULT_FILTER: &str = "warn,pawsman=info";

/// Installs the global subscriber writing to `dir/pawsman.<date>.log`; the guard flushes on drop.
pub(crate) fn install(dir: &Path, env: &EnvSnapshot) -> Result<WorkerGuard, String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("`{}`: {e}", dir.display()))?;
    let appender = Builder::new()
        .rotation(Rotation::DAILY)
        .filename_prefix("pawsman")
        .filename_suffix("log")
        .max_log_files(14)
        .build(dir)
        .map_err(|e| e.to_string())?;
    let (writer, guard) = tracing_appender::non_blocking(appender);
    let filter = env.log_filter().and_then(|f| EnvFilter::try_new(f).ok()).unwrap_or_else(|| EnvFilter::new(DEFAULT_FILTER));
    tracing_subscriber::fmt().with_writer(writer).with_ansi(false).with_env_filter(filter).try_init().map_err(|e| e.to_string())?;
    Ok(guard)
}
```

`main.rs`:

```rust
//! `pawsman`: the TUI.
#![forbid(unsafe_code)]

mod args;
mod log;

use std::io::Write as _;
use std::process::ExitCode;

use clap::Parser as _;
use pawsman_core::env::EnvSnapshot;
use pawsman_core::notice::Notice;
use pawsman_core::paths::Paths;
use pawsman_core::settings::{Animations, Settings};
use pawsman_error::ErrorInfo as _;
use pawsman_tui::{Options, SplashSetting};

fn main() -> ExitCode {
    let args = args::Args::parse();
    let env = EnvSnapshot::capture();
    let mut notices = Vec::new();
    let paths = Paths::from_env(&env).inspect_err(|e| notices.push(Notice::from_error(e))).ok();
    let _guard = paths.as_ref().and_then(|p| {
        log::install(p.log_dir(), &env).inspect_err(|e| notices.push(Notice::warn(format!("no log file: {e}")))).ok()
    });
    tracing::info!(
        version = env!("CARGO_PKG_VERSION"),
        term = env.term().unwrap_or("-"),
        tmux = env.in_tmux(),
        ssh = env.over_ssh(),
        utf8 = env.utf8_locale(),
        "start"
    );
    let settings = paths
        .as_ref()
        .map(|p| Settings::load(&p.settings_file()))
        .transpose()
        .unwrap_or_else(|e| {
            notices.push(Notice::from_error(&e));
            None
        })
        .unwrap_or_default();
    let splash = if args.no_splash || settings.ui().animations() == Animations::Off || env.no_animation() {
        SplashSetting::Off
    } else {
        SplashSetting::On
    };
    let images = args.images.unwrap_or(settings.ui().images());
    match pawsman_tui::run(Options::new(env, splash, images).notices(notices)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            tracing::error!(code = %err.code(), %err, "exit");
            exit_line(&err);
            ExitCode::FAILURE
        }
    }
}

/// The one line printed after the terminal is restored.
#[expect(clippy::disallowed_methods, reason = "the exit line after restore (rules/tui.md §1)")]
fn exit_line(err: &pawsman_tui::TuiError) {
    let help = err.help().map(|h| format!(" {h}")).unwrap_or_default();
    let _ = writeln!(std::io::stderr(), "pawsman: {}: {err}.{help}", err.code());
}
```

`tools/check-deps.sh` gains `forbid pawsman-tui tokio reqwest rhai` and `forbid pawsman-cli tokio reqwest`; `deny.toml` gains `tachyonfx` under no new rule (it already wraps ratatui) and nothing else, since no new heavy crate appears.

- [ ] **Step 4: Run all gates and the PTY tests on Linux; commit** `feat: pawsman binary, event loop and file log`.

- [ ] **Step 5: Push and watch CI on Linux and macOS.** If a PTY test fails only on macOS, read the log first; fix the cause (a `poll` difference, a `/dev/tty` behaviour), never loosen the test.

### Task 12: visual check, live matrix, docs, PR

**Files:**

- Create: `crates/pawsman-tui/src/visual.rs`, `visual/png.rs`
- Modify: `mise.toml` (cargo-insta), `README.md` (usage), `docs/DECISIONS.md`, `CLAUDE.md` (current piece), `docs/TESTING.md` (PTY harness line), `rules/tui.md` (only if behaviour drifted)

**Interfaces:**

- Produces: `#[test] #[ignore] fn visual_<scenario>()` writing `target/visual/tui-<scenario>.png`; `png::render(buffer: &Buffer, path: &Path)`.

- [ ] **Step 1: Renderer** (`visual/png.rs`, test-only, `#[cfg(test)] mod visual;` in `lib.rs`): 10×20 px cells; colours from `Color` (RGB direct; `Indexed` through the xterm palette from `views::colors`; named through the xterm defaults; `Reset` as the theme background `#161622`); block elements U+2580–U+259F and light box drawing (`─│┌┐└┘├┤┬┴┼╭╮╯╰`) drawn as rectangles; every other glyph rasterised with fontdue from the first existing font of `PAWSMAN_VISUAL_FONT`, `/usr/share/fonts/noto/NotoSansMono-Regular.ttf`, `/usr/share/fonts/TTF/DejaVuSansMono.ttf`, `/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf` (the env read takes an `#[expect(clippy::disallowed_methods, reason = "picks a font for local PNG files")]`); a missing font panics naming the paths. Bold uses the font with a one-pixel horizontal smear.

- [ ] **Step 2: Scenarios** (`visual.rs`): splash at 0, 300, 600, 900, 1200 ms (seed 42, truecolor, 80×24); splash at 600 ms in 256 colours; the static 16-colour splash; home at 80×24 and 160×48; help at 80×24; home with an error notice; home at 30×8 (wordmark replaced by the word). Run `cargo test -p pawsman-tui visual -- --ignored` and **read every PNG file**; fix what looks wrong (clipping, misalignment, colours) with a test first where the defect is checkable on `TestBackend`.

- [ ] **Step 3: Live matrix** (the visual-check skill §2 and the M0 Docker box):
  - kitty locally: `cargo run -p pawsman-cli` in a kitty window started with `~/src/github/pawsman-poc/run-kitty.sh`; screenshots with `shot.sh` at splash and home; Ctrl-Q; `stty -a` before and after match.
  - Rebuild `pawsman-poc-matrix` with the `pawsman` binary copied in (`docker build`, then `docker run -d --name pawsman-matrix -p 127.0.0.1:2222:22 …` as in M0), `ssh -t -p 2222 -i <scratchpad key> dev@127.0.0.1 ./pawsman` from kitty: splash bytes (`script -q -c ./pawsman /tmp/out` then `wc -c`), probe line from the log.
  - tmux in the box with passthrough on, off, and without `LANG`: the hints show; `probe_ms` in the log within the limits.
  - xterm on Xvfb in the box: `graphics=Sixel`, legacy Ctrl-Q quits, the splash shows indexed colours.
  - #42 real typing: type `A`, `!` and `Ж` into kitty with `wtype` (Wayland virtual keyboard, so kitty sees real key events) while `PAWSMAN_LOG=pawsman=trace` and a temporary `trace!` of decoded text inputs is enabled in a scratch build (never committed); expect `Char('A')`, `Char('!')`, `Char('Ж')` without SHIFT. If `wtype` is not installed, ask the user to type the three keys in the kitty window once.
  - Remove the container afterwards (`docker rm -f pawsman-matrix`).

- [ ] **Step 4: Docs.** `README.md`: install (`cargo install --path crates/pawsman-cli`), flags, Ctrl-Q, where the log is. `docs/DECISIONS.md` lines: own termios raw mode; pty-process and fontdue chosen over portable-pty and ab_glyph (maintenance rule); layout folding moved into 1.1 for Ctrl-Q on Russian layouts; 16-colour terminals hold the final splash frame for 1.2 s. `CLAUDE.md`: current piece 1.1 plan link. `mise.toml`: `"aqua:mitsuhiko/insta" = "1.49.0"` if `mise ls-remote aqua:mitsuhiko/insta` lists it, else `"cargo:cargo-insta" = "1.49.0"`. Update the research matrix rows in [kitty graphics over SSH](../../research/kitty-graphics-over-ssh.md#matrix) only with new measurements, not repetition.

- [ ] **Step 5: `cargo mutants --in-diff` on the branch diff**; add tests for surviving mutants that matter (decoder, caps, probe reader); record the rest with reasons in the PR.

- [ ] **Step 6: PR and merge.** PR body: summary, the new dependencies and why, accepted snapshots, the PNG files reviewed, the matrix results, the test changes (none expected), `Closes #10, Closes #42`, and `Refs #14` (images remain for 1.6). Merge when CI is green; delete the branch; tick the plan's boxes in a follow-up commit only if the plan is kept as a record.
