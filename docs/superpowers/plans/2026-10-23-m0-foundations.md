# M0 Foundations Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A pawsman repository with semoxide-grade tooling, rules and docs, the first crate (`pawsman-error`), GitHub milestones and issues for M0–M7, and four throwaway PoCs that settle the terminal and scripting unknowns before M1.

**Architecture:** Cargo workspace under `crates/`, configs adapted from semoxide (the stricter repo) and TUI rules adapted from b24-experiment. PoCs live in a separate local repo, `~/src/github/pawsman-poc`; only their findings come back, as `docs/research/*.md`.

**Tech Stack:** Rust 1.99.0 (MSRV 1.97, edition 2024), mise, lefthook, nextest, cargo-deny, typos, gitleaks, markdownlint-cli2, actionlint, zizmor, GitHub Actions. PoCs: ratatui, crossterm, ratatui-image, tachyonfx, image, rhai.

**Spec:** [docs/superpowers/specs/2026-10-23-pawsman-design.md](../specs/2026-10-23-pawsman-design.md)

## Global Constraints

- Toolchain `1.99.0` in `rust-toolchain.toml`; `rust-version = "1.97"`; `edition = "2024"`; `license = "MIT OR Apache-2.0"`.
- Every dependency pinned with `=x.y.z` in `[workspace.dependencies]`; crates use `dep.workspace = true` and `[lints] workspace = true`.
- Workspace lints copied verbatim from semoxide's `Cargo.toml` (`[workspace.lints.rust]` and `[workspace.lints.clippy]`); never loosened.
- No `unwrap`/`expect`/`panic!` outside tests; `#[expect(lint, reason = "…")]`, never `#[allow]`; `#![forbid(unsafe_code)]` in every crate root.
- No `anyhow`, no `thiserror`: hand-written `Display` and `std::error::Error`.
- Docs short, technical, Mermaid for diagrams, never ASCII art; one paragraph per line; comments never describe history.
- Conventional Commits; hooks never skipped (`--no-verify` forbidden); commits use the repo-local identity `smsteel <sm.steel@hotmail.com>`, never a `-c user.email` override.
- Branch per change, never commit to `main` after the root commit; PRs merge into `main`.
- Tests first: a `test:` commit failing on assertions, then a stop for the user's approval, then `feat:`/`fix:`.
- Dependencies beyond those listed in this plan need the user's approval first.
- PoC code is throwaway: it never moves into `crates/`; its findings do.

## Review Focus

1. A terminal that never answers the capability probe (some multiplexers, a slow SSH link): the probe must time out, fall back, and leave no reply bytes for the key handler. Pinned by PoC 1, Task 6 step 3.
2. tmux without `allow-passthrough`: kitty images silently vanish. Expected: the probe sees no graphics reply and falls back to half blocks. Pinned by PoC 1, Task 6 step 6.
3. A panic while kitty keyboard flags are pushed leaves the user's shell unusable. Expected: the panic hook pops the flags and restores the screen. Pinned by PoC 2, Task 7 step 3.
4. A Rhai script with an infinite loop or a huge allocation hangs or exhausts memory. Expected: a typed limit error within the time budget. Pinned by PoC 3, Task 8 steps 1–2.
5. A splash on a terminal smaller than the logo or without truecolor draws garbage. Expected: no splash below the logo size; indexed colours only without truecolor. Pinned by PoC 4, Task 9 step 1.

---

### Task 1: Workspace, tooling configs and `pawsman-error`

**Files:**
- Create: `Cargo.toml`, `clippy.toml`, `rustfmt.toml`, `rust-toolchain.toml`, `deny.toml`, `mise.toml`, `lefthook.yml`, `.config/nextest.toml`, `.gitleaks.toml`, `.markdownlint.json`, `.gitattributes`, `.gitignore`
- Create: `crates/pawsman-error/Cargo.toml`, `crates/pawsman-error/src/lib.rs`

**Interfaces:**
- Produces: `pawsman_error::{ErrorCode, Retry, ErrorInfo}`. `ErrorCode::new(&'static str) -> ErrorCode` (const), `ErrorCode::as_str(self) -> &'static str`, `ErrorCode::namespace(self) -> &'static str`, `ErrorCode::slug(self) -> &'static str`, `Display`. `Retry::{Retryable, Permanent}`. `trait ErrorInfo: std::error::Error { fn code(&self) -> ErrorCode; fn help(&self) -> Option<&'static str> { None } fn retry(&self) -> Retry { Retry::Permanent } }`.

- [ ] **Step 1: Branch**

```bash
cd ~/src/github/pawsman && git switch main && git switch -c chore/workspace-tooling
```

- [ ] **Step 2: Copy configs from semoxide unchanged**

```bash
S=~/src/github/semoxide
cp $S/rustfmt.toml $S/rust-toolchain.toml $S/.gitattributes $S/.markdownlint.json .
mkdir -p .config && cp $S/.config/nextest.toml .config/
```

- [ ] **Step 3: Write `.gitignore`**

```gitignore
target/
*.snap.new
*.pending-snap
mutants.out*/
lcov.info
```

- [ ] **Step 4: Write `.gitleaks.toml`**

```toml
# Test fixtures use the fake-secret marker (TESTING.md); only that marker is allowlisted.
[extend]
useDefault = true

[[allowlists]]
description = "fake test secrets containing PAWSMAN_FAKE"
regexes = ['''PAWSMAN_FAKE''']
```

- [ ] **Step 5: Write `Cargo.toml`**

Copy `$S/Cargo.toml`, then change it to this header and dependency section; the `[workspace.lints.*]` and `[profile.*]` tables stay byte-identical to semoxide's except the `unsafe_code` comment.

```toml
[workspace]
resolver = "3"
members = ["crates/*"]

[workspace.package]
version = "0.0.0"
edition = "2024"
rust-version = "1.97"
license = "MIT OR Apache-2.0"
repository = "https://github.com/sm-steel/pawsman"
publish = false

[workspace.dependencies]
# Internal crates (CODE-ARCHITECTURE §1); external dependencies are added by the tasks that need them.
pawsman-error = { path = "crates/pawsman-error" }

[workspace.lints.rust]
# Every crate root adds `#![forbid(unsafe_code)]`.
unsafe_code = "deny"
missing_docs = "warn"
unreachable_pub = "warn"
```

`default-members` is added in M1 with `pawsman-cli`.

- [ ] **Step 6: Write `clippy.toml`**

Copy `$S/clippy.toml`. Replace the `disallowed-methods` reasons that name semoxide documents and crates:

```toml
disallowed-methods = [
  { path = "std::env::var", reason = "only the env snapshot reads the process env (OBSERVABILITY P1)" },
  { path = "std::env::var_os", reason = "only the env snapshot reads the process env (OBSERVABILITY P1)" },
  { path = "std::env::vars", reason = "only the env snapshot reads the process env (OBSERVABILITY P1)" },
  { path = "std::env::vars_os", reason = "only the env snapshot reads the process env (OBSERVABILITY P1)" },
  { path = "std::env::temp_dir", reason = "use tempfile, never a shared temp dir (CODE-ARCHITECTURE P2)" },
  { path = "std::process::Command::new", reason = "only the $EDITOR launcher spawns processes (CODE-ARCHITECTURE §2)" },
  { path = "tokio::process::Command::new", reason = "only the $EDITOR launcher spawns processes (CODE-ARCHITECTURE §2)", allow-invalid = true },
  { path = "std::io::stdout", reason = "the terminal belongs to term::Term (rules/tui.md §1)" },
  { path = "std::io::stderr", reason = "the terminal belongs to term::Term (rules/tui.md §1)" },
  { path = "reqwest::ClientBuilder::danger_accept_invalid_certs", reason = "TLS checks stay on unless the user disables them per collection (rules/rust-api-client.md)", allow-invalid = true },
  { path = "reqwest::ClientBuilder::danger_accept_invalid_hostnames", reason = "TLS checks stay on unless the user disables them per collection (rules/rust-api-client.md)", allow-invalid = true },
]
```

Keep every other line of semoxide's file.

- [ ] **Step 7: Write `deny.toml`**

Copy `$S/deny.toml`. Replace `[bans].deny` with pawsman's boundaries, and drop semoxide's `allow-git`:

```toml
deny = [
  # Crate boundaries (CODE-ARCHITECTURE §2): heavy dependencies only through their owning crate.
  { crate = "tokio", wrappers = ["pawsman-net", "pawsman-grpc", "pawsman-mock", "reqwest", "hyper", "hyper-util", "h2", "tower", "tonic", "tokio-util", "tokio-stream", "tokio-rustls", "tokio-tungstenite", "axum"], reason = "async only on the net runtime thread (CODE-ARCHITECTURE §3)" },
  { crate = "ratatui", wrappers = ["pawsman-tui", "tachyonfx", "ratatui-image", "tui-big-text"], reason = "drawing only in pawsman-tui" },
  { crate = "crossterm", wrappers = ["pawsman-tui", "ratatui", "ratatui-crossterm"], reason = "terminal access only in pawsman-tui" },
  { crate = "rhai", wrappers = ["pawsman-script"], reason = "scripting only through pawsman-script" },
  { crate = "tonic", wrappers = ["pawsman-grpc"], reason = "gRPC only in pawsman-grpc" },
  { crate = "openssl-sys", reason = "rustls only" },
  { crate = "anyhow", reason = "typed errors per crate (CODE-ARCHITECTURE §4)" },
]
```

Wrappers for crates that do not exist yet are harmless.

- [ ] **Step 8: Write `mise.toml`**

```toml
# Dev tools pinned per project; `mise install` sets them up locally, CI installs from this file too.
# Renovate bumps these versions. Rust itself is pinned in rust-toolchain.toml.
[tools]
"aqua:crate-ci/typos" = "1.51.1"
"aqua:evilmartians/lefthook" = "2.1.17"
"github:nextest-rs/nextest" = { version = "0.9.146", version_prefix = "cargo-nextest-" }
"aqua:EmbarkStudios/cargo-deny" = "0.20.2"
"github:sourcefrog/cargo-mutants" = "27.1.0"
"github:taiki-e/cargo-hack" = "0.6.45"
"aqua:gitleaks/gitleaks" = "8.30.1"
"aqua:rhysd/actionlint" = "1.7.12"
"aqua:zizmorcore/zizmor" = "1.30.1"
# Node LTS runs markdownlint-cli2.
node = "24.21.0"
"npm:markdownlint-cli2" = "0.23.3"
```

The insta CLI is added in M1 together with the `insta` crate, at the same version.

- [ ] **Step 9: Write `lefthook.yml`**

Copy `$S/lefthook.yml`, delete the `semgrep` pre-push command, and keep the `nextest` skip on a missing `crates/`.

- [ ] **Step 10: Install tools and hooks**

Run: `mise install && lefthook install`
Expected: every tool reports installed; `sync hooks: ✔️ (pre-commit, pre-push)`.

- [ ] **Step 11: Write `crates/pawsman-error/Cargo.toml`**

```toml
[package]
name = "pawsman-error"
description = "Error codes and the ErrorInfo trait shared by every pawsman crate"
version.workspace = true
edition.workspace = true
rust-version.workspace = true
license.workspace = true
publish.workspace = true

[lints]
workspace = true
```

- [ ] **Step 12: Write the failing tests**

`crates/pawsman-error/src/lib.rs`, with the types declared but `namespace` and `slug` returning `""`:

```rust
//! Error metadata shared by every pawsman crate.
//!
//! Each crate keeps its own typed error and implements [`ErrorInfo`] on it, so the edge (TUI notices,
//! `pawsman run` reports) can render a message, a help line and decide whether to retry.
#![forbid(unsafe_code)]

use std::fmt;

/// A namespaced, stable error code such as `net::tls_handshake`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ErrorCode(&'static str);

impl ErrorCode {
    /// Creates a code; use one constant per failure in the owning crate's `codes` module.
    #[must_use]
    pub const fn new(code: &'static str) -> Self {
        Self(code)
    }

    /// The code as text.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        self.0
    }

    /// The part before `::`, which names the docs folder (`docs/errors/<namespace>/`).
    #[must_use]
    pub fn namespace(self) -> &'static str {
        ""
    }

    /// The part after `::`, which names the docs page (`docs/errors/<namespace>/<slug>.md`).
    #[must_use]
    pub fn slug(self) -> &'static str {
        ""
    }
}

impl fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}

/// Whether repeating the same operation later may succeed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Retry {
    /// Transient: network, rate limit, server restart.
    Retryable,
    /// Permanent until the input changes.
    Permanent,
}

/// Metadata every error in the workspace exposes to the edge.
pub trait ErrorInfo: std::error::Error {
    /// Stable machine-readable code.
    fn code(&self) -> ErrorCode;

    /// One imperative sentence telling the user what to do, if there is something to do.
    fn help(&self) -> Option<&'static str> {
        None
    }

    /// Whether the operation may succeed if repeated.
    fn retry(&self) -> Retry {
        Retry::Permanent
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn code_displays_as_text() {
        let code = ErrorCode::new("net::tls_handshake");

        let shown = code.to_string();

        assert_eq!(shown, "net::tls_handshake", "Display is the raw code");
    }

    #[test]
    fn code_splits_into_namespace_and_slug() {
        let code = ErrorCode::new("model::unknown_var");

        let parts = (code.namespace(), code.slug());

        assert_eq!(parts, ("model", "unknown_var"), "split at the first `::`");
    }

    #[test]
    fn code_without_separator_is_all_slug() {
        let code = ErrorCode::new("internal");

        let parts = (code.namespace(), code.slug());

        assert_eq!(parts, ("", "internal"), "no namespace when there is no `::`");
    }

    #[derive(Debug)]
    struct Fake;

    impl fmt::Display for Fake {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("fake")
        }
    }

    impl std::error::Error for Fake {}

    impl ErrorInfo for Fake {
        fn code(&self) -> ErrorCode {
            ErrorCode::new("test::fake")
        }
    }

    #[test]
    fn defaults_are_no_help_and_permanent() {
        let err = Fake;

        let meta = (err.help(), err.retry());

        assert_eq!(meta, (None, Retry::Permanent), "trait defaults");
    }
}
```

- [ ] **Step 13: Run the tests to verify they fail**

Run: `cargo nextest run -p pawsman-error`
Expected: FAIL in `code_splits_into_namespace_and_slug` and `code_without_separator_is_all_slug` (left `("", "")`); the other two pass.

- [ ] **Step 14: Commit the tests and the tooling, then stop for approval**

```bash
git add -A
git commit -m "test: ErrorCode namespace and slug split, against a stub

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

Stop and ask the user to approve the tests before Step 15.

- [ ] **Step 15: Implement**

Replace the two stub bodies:

```rust
    pub fn namespace(self) -> &'static str {
        self.0.split_once("::").map_or("", |(namespace, _)| namespace)
    }

    pub fn slug(self) -> &'static str {
        self.0.split_once("::").map_or(self.0, |(_, slug)| slug)
    }
```

- [ ] **Step 16: Run the gates**

Run: `cargo nextest run --workspace && CARGO_BUILD_WARNINGS=deny cargo clippy --workspace --all-targets --locked && cargo fmt --all --check && cargo deny check`
Expected: 4 tests pass; clippy, fmt and deny clean.

- [ ] **Step 17: Commit**

```bash
git add -A
git commit -m "feat: ErrorCode namespace and slug for docs page lookup

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 2: Rules, agent docs, project docs and licences

**Files:**
- Create: `README.md`, `AGENTS.md`, `CLAUDE.md`, `LICENSE-MIT`, `LICENSE-APACHE`
- Create: `rules/rust-project.md`, `rules/tui.md`, `rules/rust-api-client.md`
- Create: `docs/ARCHITECTURE.md`, `docs/CODE-ARCHITECTURE.md`, `docs/DECISIONS.md`, `docs/TESTING.md`, `docs/OBSERVABILITY.md`, `docs/RUST-PITFALLS.md`, `docs/errors/README.md`
- Create: `.claude/skills/rust-testing/SKILL.md`, `.claude/skills/rust-async/SKILL.md`, `.claude/skills/visual-check/SKILL.md`

**Interfaces:**
- Consumes: section numbers used by Task 1's `clippy.toml` reasons: OBSERVABILITY P1 (env snapshot), CODE-ARCHITECTURE P2 (tempfile), CODE-ARCHITECTURE §2 (boundaries), §3 (async), §4 (errors), rules/tui.md §1.
- Produces: the docs index every later task links to.

- [ ] **Step 1: Licences**

```bash
cp ~/src/github/semoxide/LICENSE-APACHE .
sed 's/The semoxide contributors/The pawsman contributors/' ~/src/github/semoxide/LICENSE-MIT > LICENSE-MIT
```

- [ ] **Step 2: `rules/rust-project.md`**

Copy `~/src/gitlab/b24-experiment/rules/rust-project.md`. Remove every *(adapt)* passage about the chat client and Bitrix24, replace the b24 crate table with the spec's §1 table, and replace `B24_` names with `PAWSMAN_` (`PAWSMAN_LOG`, `PAWSMAN_FAKE`). Keep the header line "Conventions extracted from semoxide; they win every conflict."

- [ ] **Step 3: `rules/tui.md`**

Copy `~/src/gitlab/b24-experiment/rules/tui.md` and keep sections 1–7, 9 and 11 with b24 names replaced (`b24-tui` → `pawsman-tui`, `b24_core` → `pawsman_core`, log file `pawsman.log`). Then:
- In §5, replace "Ctrl-C quits like Ctrl-Q" with: "Ctrl-Q quits. Ctrl-C copies the selection and does nothing (with a hint naming Ctrl-Q) when nothing is selected. Ctrl-V pastes through bracketed paste. Raw mode clears IXON so Ctrl-Q arrives as a key."
- Drop the composer, sign-in, send bar, file bar, portal and chat-specific bullets; §8 keeps only: "Copy goes out as OSC 52 through `Term::copy` (works in kitty and over SSH); arboard runs on its own thread and only in local sessions."
- In §9, replace the XTVERSION allowlist with the spec's probe table (§3, Capability probe), and add: "Inside tmux, graphics and OSC 52 are wrapped in DCS passthrough."
- Add §12 **Startup animation**: the bullets of the spec's "Startup animation" section.
- §11 checks become: `cargo test -p pawsman-tui`, `cargo test -p pawsman-tui visual -- --ignored`, and the PTY probe tests.

- [ ] **Step 4: `rules/rust-api-client.md`**

New file, one numbered list:

```markdown
# API client rules

Rules for request execution, storage and secrets. [rust-project.md](rust-project.md) wins every conflict.

1. **Threads.** The main loop owns the terminal; the engine thread owns `AppState`; the net thread owns the tokio runtime. They talk only through `Command`, `Request` and `Event` enums over channels (CODE-ARCHITECTURE §3).
2. **Requests are values.** Interpolation is a pure function from a request and its scopes to a resolved request plus a trace; the net layer only sends resolved requests.
3. **Unknown variables fail.** An unresolved `{{name}}` is `model::unknown_var`, never an empty string.
4. **Secrets never touch disk in plain text.** They are `secrecy::SecretString` from resolution to the wire; collection files hold names only; history, logs and cURL export redact them unless the user asks to include them.
5. **TLS checks stay on.** Disabling certificate checks is a per-collection setting the user sets explicitly; the danger methods are banned elsewhere (clippy.toml).
6. **Cancel everything.** Every in-flight request, stream and connection has a cancellation token owned by the engine; quitting cancels and joins them.
7. **Bodies are bounded.** Response bodies stream into memory up to a cap and spill to a temp file beyond it; history keeps at most the cap.
8. **Files are the source of truth.** The store writes atomically (temp file plus rename) and the watcher reloads external edits; an unparsable file shows as an error node, never a crash.
```

- [ ] **Step 5: `AGENTS.md`**

Copy `~/src/github/semoxide/AGENTS.md` and replace `semoxide` with `pawsman`; the skills list names `rust-testing`, `rust-async` and `visual-check`.

- [ ] **Step 6: `CLAUDE.md`**

Copy `~/src/github/semoxide/CLAUDE.md` and adapt it:
- Header: "pawsman: a pure-Rust terminal API client inspired by Bruno. Phase: M0 foundations."
- Hard requirements: Rust only (no JavaScript, no wasm); kitty protocols first-class; everything works over SSH and tmux; "Library first, thin CLI" becomes "Engine first, thin TUI and CLI".
- Add the rules table: `rules/rust-project.md` (wins every conflict), `rules/tui.md`, `rules/rust-api-client.md`.
- Add "Verifying UI changes: use the visual-check skill."
- Add "GitHub milestones and issues: create, update and close them without asking; a milestone is never created for a single issue."
- Index: one line per file created in this task, plus `docs/re/bruno-feature-inventory.md`, `docs/research/startup-animation.md`, the spec and this plan.
- Tools list: the entries of `mise.toml`.

- [ ] **Step 7: `README.md`**

```markdown
# pawsman

A terminal API client inspired by [Bruno](https://github.com/usebruno/bruno): offline, file-based, git-friendly collections in a fast TUI. Pure Rust, kitty graphics and keyboard protocols first, and it works the same over SSH.

> **Status:** M0, foundations. Nothing to run yet.

## Development

1. Install [rustup](https://rustup.rs) and [mise](https://mise.jdx.dev).
2. `mise install` installs the pinned dev tools; `lefthook install` sets up the git hooks.
3. Read [AGENTS.md](AGENTS.md) before contributing.

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or [MIT license](LICENSE-MIT) at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in pawsman by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.
```

- [ ] **Step 8: Project docs**

Each file is short and links to the spec instead of repeating it:
- `docs/ARCHITECTURE.md`: the spec's crates table and thread diagram, the collection layout, and the variable scope order, each as one section.
- `docs/CODE-ARCHITECTURE.md`: §1 crate split rule ("along purity and heavy dependencies"), §2 boundaries (the `deny.toml` wrappers and `tools/check-deps.sh`, and that only the `$EDITOR` launcher spawns processes), §3 async (private runtime, no public `async fn`, cancellation), §4 errors (struct over private `Kind`, `ErrorInfo`, codes in `codes.rs` with `codes::ALL`), modules (`foo.rs` + `foo/`, no `mod.rs`); pitfall rows P1 env snapshot, P2 tempfile only.
- `docs/OBSERVABILITY.md`: P1 env snapshot; tracing only in libraries; the file log under `~/.local/state/pawsman/`; `PAWSMAN_LOG` then `RUST_LOG`; `url_for_log()`.
- `docs/TESTING.md`: the spec's Testing section, plus the `PAWSMAN_FAKE` marker and `NEXTEST_PROFILE=ci`.
- `docs/DECISIONS.md`: one bullet per row of the spec's "What the user chose" table, format `**X** — reason (→ link)`.
- `docs/RUST-PITFALLS.md`: copy semoxide's table, keeping only rows that are not semoxide-specific.
- `docs/errors/README.md`: copy semoxide's wording rules and page template, renamed.

- [ ] **Step 9: Skills**

```bash
mkdir -p .claude/skills
cp -r ~/src/github/semoxide/.claude/skills/rust-testing ~/src/github/semoxide/.claude/skills/rust-async .claude/skills/
cp -r ~/src/gitlab/b24-experiment/.claude/skills/visual-check .claude/skills/
grep -rl -e semoxide -e b24 .claude/skills
```

Edit every file the grep lists: crate and binary names to pawsman's; the visual command to `cargo test -p pawsman-tui visual -- --ignored` with PNGs in `target/visual/tui-*.png`.

- [ ] **Step 10: Verify docs**

Run: `markdownlint-cli2 "**/*.md" && typos && grep -rn -i -e semoxide -e b24 -e bitrix --include='*.md' . | grep -v -e 'rules/rust-project.md' -e 'docs/RUST-PITFALLS.md' -e 'docs/superpowers/'`
Expected: lint and typos clean; the grep prints nothing (only attribution lines in the two named files mention semoxide).

- [ ] **Step 11: Commit**

```bash
git add -A
git commit -m "docs: rules, agent guide, project docs and licences

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 3: Boundary check, CI and the first push

**Files:**
- Create: `tools/check-deps.sh`, `.github/workflows/ci.yml`, `.github/workflows/deny-daily.yml`, `renovate.json`

**Interfaces:**
- Consumes: the workspace from Task 1.
- Produces: the `required-checks` job that branch protection requires.

- [ ] **Step 1: `tools/check-deps.sh`**

```bash
#!/usr/bin/env bash
# Crate boundaries (docs/CODE-ARCHITECTURE.md §2): the core never links a terminal library, and
# heavy dependencies stay in their owning crate. Exits non-zero and names the offending crate.
set -euo pipefail
cd "$(dirname "$0")/.."

has() { cargo metadata --format-version 1 --no-deps | grep -q "\"name\":\"$1\""; }

forbid() { # forbid <package> <crate>...
  local pkg=$1; shift
  has "$pkg" || return 0
  local tree
  tree=$(cargo tree -p "$pkg" -e normal --prefix none --format '{p}' | awk '{print $1}' | sort -u)
  for bad in "$@"; do
    if grep -qx "$bad" <<<"$tree"; then
      echo "boundary violation: $pkg depends on $bad" >&2
      exit 1
    fi
  done
}

forbid pawsman-error serde tokio
forbid pawsman-model tokio reqwest ratatui crossterm rhai
forbid pawsman-script tokio reqwest ratatui crossterm
forbid pawsman-import tokio reqwest ratatui crossterm rhai
forbid pawsman-core ratatui crossterm tachyonfx ratatui-image
forbid pawsman-net ratatui crossterm rhai
echo "crate boundaries ok"
```

Run: `chmod +x tools/check-deps.sh && tools/check-deps.sh`
Expected: `crate boundaries ok`.

- [ ] **Step 2: `.github/workflows/ci.yml`**

Copy `~/src/github/semoxide/.github/workflows/ci.yml` and change it:
- Delete the semgrep steps and the Windows matrix entry; the test matrix is `ubuntu-latest` and `macos-latest`.
- Replace the `cargo insta test …` step with `cargo nextest run --workspace --profile ci --no-tests=pass` until insta arrives in M1.
- Add `tools/check-deps.sh` to the `quality` job after clippy.
- Keep the SHA-pinned actions, `persist-credentials: false`, `permissions: contents: read`, concurrency, `CARGO_BUILD_WARNINGS: deny`, the `detect`, `deny`, `msrv` and `required-checks` jobs.

Copy `deny-daily.yml` unchanged.

- [ ] **Step 3: `renovate.json`**

semoxide's preset lives in the semoxide org, so inline it. Copy `~/src/gitlab/b24-experiment/rules/semoxide-reference/renovate/default.json` to `renovate.json`, adding `"$schema": "https://docs.renovatebot.com/renovate-schema.json"` and `"mode": "full"`.

- [ ] **Step 4: Lint workflows**

Run: `actionlint && zizmor --offline .github/workflows`
Expected: no findings.

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "ci: quality, deny, test matrix, msrv and crate boundary check

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

- [ ] **Step 6: Push and open the PRs**

```bash
git push -u origin main
git push -u origin docs/program-design chore/workspace-tooling
gh pr create --base main --head docs/program-design --title "docs: program design, Bruno inventory, M0 plan" --body "Program design spec, rea-derived Bruno feature inventory, startup animation research and the M0 plan.

🤖 Generated with [Claude Code](https://claude.com/claude-code)"
```

Merge the docs PR first after the user's review, then rebase `chore/workspace-tooling` on `main` and open its PR (body: the list of files, and "Closes #<Task 1 issue>" from Task 4).

- [ ] **Step 7: Watch CI**

Run: `gh pr checks --watch`
Expected: every job green, `required-checks` passes. A red job is fixed in the code, never by loosening a rule.

- [ ] **Step 8: Protect `main`**

```bash
gh api -X PUT repos/sm-steel/pawsman/branches/main/protection --input - <<'EOF'
{"required_status_checks":{"strict":true,"contexts":["required-checks"]},"enforce_admins":false,"required_pull_request_reviews":null,"restrictions":null}
EOF
```

Expected: JSON echoing the protection. A private repo on a free plan returns 403: then record "branch protection unavailable" in `docs/DECISIONS.md` and continue.

---

### Task 4: GitHub milestones, labels and issues

**Files:** none (GitHub only).

**Interfaces:**
- Produces: milestone numbers and issue numbers that later PRs close.

- [ ] **Step 1: Labels**

```bash
for l in "area:model" "area:tui" "area:net" "area:script" "area:import" "area:core" "area:cli" "area:tooling" "area:docs" "needs-decision" "blocked-external" "poc"; do
  gh label create "$l" --repo sm-steel/pawsman --force
done
```

- [ ] **Step 2: Milestones**

```bash
while IFS='|' read -r title desc; do
  gh api repos/sm-steel/pawsman/milestones -f title="$title" -f description="$desc"
done <<'EOF'
M0 Foundations|Tooling, rules, docs, pawsman-error, PoCs for kitty images, keyboard, OSC 52, Rhai, splash
M1 HTTP client|Startup animation, model and format, environments, store, three-pane TUI, response viewer, history, cURL
M2 Tests and runner|Captures, assertions, Rhai sandbox, chaining, runner, pawsman run
M3 Auth and network|Auth schemes, OAuth2, cookies, proxies, mTLS, layered secrets
M4 Protocols|GraphQL, SSE, WebSocket, gRPC
M5 Importers|Bruno, Postman v2.1, OpenAPI 3 and Swagger 2
M6 Mock and drift|pawsman mock, OpenAPI drift and sync
M7 Git and AI|Git through gix, AI request and Rhai generation
EOF
```

- [ ] **Step 3: M0 issues**

One issue per Task 1–3 and 5–10 of this plan, with milestone `M0 Foundations`, the task title as the issue title, an "Approach:" line taken from the task's first sentence, and labels `area:tooling`, `area:docs` or `poc`.

```bash
gh issue create --repo sm-steel/pawsman --milestone "M0 Foundations" --label area:tooling \
  --title "Workspace, tooling configs and pawsman-error" \
  --body "Approach: semoxide configs adapted, ErrorCode namespace and slug test-first. Plan: docs/superpowers/plans/2026-10-23-m0-foundations.md Task 1."
```

Repeat for each task.

- [ ] **Step 4: M1–M7 epics**

One issue per milestone, labelled by area, listing the milestone's "Delivers" cell from the spec as a task list (`- [ ] …`). Each epic is broken into issues when its milestone spec is written.

- [ ] **Step 5: Verify**

Run: `gh api repos/sm-steel/pawsman/milestones --jq '.[] | "\(.title): \(.open_issues)"'`
Expected: M0 with 9 open issues, M1–M7 with 1 each.

---

### Task 5: PoC repository

**Files:**
- Create: `~/src/github/pawsman-poc/` (separate local git repo, not pushed unless the user asks), `Cargo.toml`, `rust-toolchain.toml`, `README.md`

- [ ] **Step 1: Create**

```bash
mkdir -p ~/src/github/pawsman-poc && cd ~/src/github/pawsman-poc && git init -b main
git config user.name smsteel && git config user.email sm.steel@hotmail.com
cp ~/src/github/pawsman/rust-toolchain.toml .
```

- [ ] **Step 2: Resolve the newest compatible versions**

Run: `cargo info ratatui && cargo info crossterm && cargo info ratatui-image && cargo info tachyonfx && cargo info image && cargo info rhai`
Pick ratatui 0.30.x, the crossterm version ratatui 0.30 depends on, ratatui-image 11.1.0, tachyonfx 0.25.2, the newest image and rhai. Confirm ratatui-image's feature names in `cargo info ratatui-image` (the crossterm backend and PNG decoding, never `chafa-dyn`) and crossterm's `osc52` feature. Record each in the README.

- [ ] **Step 3: `Cargo.toml`**

```toml
[workspace]
resolver = "3"
members = ["probe", "keys", "rhai-limits", "splash"]

[workspace.package]
edition = "2024"
publish = false

[workspace.dependencies]
ratatui = "=0.30.<x>"
crossterm = { version = "=<ratatui's crossterm>", features = ["osc52"] }
ratatui-image = { version = "=11.1.0", default-features = false, features = ["crossterm", "image-defaults"] }
tachyonfx = { version = "=0.25.2", default-features = false, features = ["std"] }
image = { version = "=<newest>", default-features = false, features = ["png"] }
rhai = { version = "=<newest>", features = ["sync"] }
```

Replace each `<…>` with the version Step 2 recorded before saving.

- [ ] **Step 4: README and commit**

`README.md`: "Throwaway PoCs for pawsman M0. Findings go to pawsman's docs/research/. Nothing here is product code." plus the version list.

```bash
git add -A && git commit -m "chore: poc workspace"
```

---

### Task 6: PoC 1, capability probe and kitty images over SSH and tmux

**Files:**
- Create: `~/src/github/pawsman-poc/probe/Cargo.toml`, `probe/src/main.rs`, `probe/src/parse.rs`, `probe/assets/logo.png`
- Create (pawsman): `docs/research/kitty-graphics-over-ssh.md`

**Interfaces:**
- Produces (for M1, as a design input only): `parse::Caps { kitty_graphics: bool, sixel: bool, kitty_keyboard: bool, cell_px: Option<(u16, u16)> }` and `parse::parse(replies: &[u8]) -> Caps`.

- [ ] **Step 1: Write the parser tests**

`probe/src/parse.rs`:

```rust
//! Pure parser for the terminal's replies to the capability probe.

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Caps {
    pub kitty_graphics: bool,
    pub sixel: bool,
    pub kitty_keyboard: bool,
    pub cell_px: Option<(u16, u16)>,
}

pub fn parse(_replies: &[u8]) -> Caps {
    Caps::default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kitty_answers_everything() {
        let replies = b"\x1b_Gi=31;OK\x1b\\\x1b[?1u\x1b[6;20;10t\x1b[?62;4;22c";

        let caps = parse(replies);

        assert_eq!(caps, Caps { kitty_graphics: true, sixel: true, kitty_keyboard: true, cell_px: Some((10, 20)) });
    }

    #[test]
    fn only_da1_means_nothing_but_what_da1_says() {
        let replies = b"\x1b[?62;22c";

        let caps = parse(replies);

        assert_eq!(caps, Caps::default());
    }

    #[test]
    fn graphics_error_is_not_support() {
        let replies = b"\x1b_Gi=31;ENOTSUPPORTED:no\x1b\\\x1b[?62c";

        let caps = parse(replies);

        assert!(!caps.kitty_graphics, "only OK counts");
    }
}
```

- [ ] **Step 2: Run to fail, then implement**

Run: `cargo test -p probe` → `kitty_answers_everything` fails.
Implement `parse` by scanning for: APC `\x1b_G` … `;OK\x1b\\` with `i=31` (graphics), `\x1b[?<flags>u` (keyboard), `\x1b[6;<h>;<w>t` (cell pixels, height first), and DA1 `\x1b[?…c` whose `;`-separated attributes include `4` (sixel). Run again → 3 pass.

- [ ] **Step 3: Probe with timeout, swallowing late replies**

`probe/src/main.rs`: enable raw mode; write `\x1b_Gi=31,s=1,v=1,a=q,t=d,f=24;AAAA\x1b\\`, `\x1b[?u`, `\x1b[16t`, `\x1b[c` (DA1 last, as terminator) to `/dev/tty`; read from `/dev/tty` until the DA1 reply or 1000 ms; after a timeout keep reading for 150 ms and discard. Print `Caps` after restoring the terminal. Test the timeout path with `script -qc 'cargo run -p probe' /dev/null < /dev/null` (no terminal answers).
Expected: returns after about 1.15 s with `Caps::default()`, nothing echoed.

- [ ] **Step 4: Image through ratatui-image**

After the probe, build `ratatui_image::picker::Picker::from_query_stdio()` before any frame, load `assets/logo.png` (any 256×256 PNG), render a `StatefulImage` in a bordered block, quit on Ctrl-Q.

- [ ] **Step 5: Run the matrix**

| # | Where | Command |
|---|---|---|
| 1 | kitty local | `cargo run -p probe` |
| 2 | kitty over SSH | `ssh -t localhost ~/src/github/pawsman-poc/target/debug/probe` |
| 3 | kitty, tmux, passthrough on | `tmux new 'tmux set -g allow-passthrough on; ~/…/probe'` |
| 4 | kitty, tmux, passthrough off | `tmux new '~/…/probe'` |
| 5 | kitty, SSH then tmux | `ssh -t localhost tmux new ~/…/probe` |
| 6 | a non-kitty terminal (foot or alacritty, whichever is installed) | `probe` |

For each, record the `Caps`, the protocol ratatui-image picked (`picker.protocol_type()`), whether the image showed, and the bytes sent (`strace -e write -o /tmp/… probe`, sum of write sizes).

- [ ] **Step 6: Check the tmux-off fallback**

Expected in row 4: graphics reply missing, half blocks drawn, no garbage. If kitty graphics are reported and the image is missing, that is a finding: the probe must then also check `allow-passthrough` via `tmux show -gv allow-passthrough`.

- [ ] **Step 7: Findings**

Write `pawsman/docs/research/kitty-graphics-over-ssh.md`: the matrix with results, the bytes per image, whether ratatui-image's kitty path is usable as the M1 implementation or pawsman needs its own (b24's `gfx` approach), and the probe timeouts that worked over SSH. Commit in pawsman: `docs: kitty graphics over SSH and tmux findings`. Close the issue.

---

### Task 7: PoC 2, kitty keyboard protocol, Ctrl-Q and OSC 52

**Files:**
- Create: `~/src/github/pawsman-poc/keys/Cargo.toml`, `keys/src/main.rs`, `keys/src/term.rs`
- Create (pawsman): `docs/research/keyboard-and-clipboard.md`

- [ ] **Step 1: Mode stack with undo**

`keys/src/term.rs`: `Term::setup()` enables raw mode, alternate screen, then (only if `crossterm::terminal::supports_keyboard_enhancement()?`) pushes `KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES | REPORT_ALL_KEYS_AS_ESCAPE_CODES | REPORT_EVENT_TYPES`, then `EnableBracketedPaste`, `EnableFocusChange`, `EnableMouseCapture`. Each step pushes its undo onto a `Vec<Mode>`; `restore()` pops newest first; `Drop` calls `restore()`; a panic hook calls the same restore before the default hook.

- [ ] **Step 2: Key log**

`keys/src/main.rs`: a ratatui list of the last 30 `crossterm::event::Event`s with their `KeyEventKind`. Ctrl-Q quits. Ctrl-C with a "selection" (the last logged line) writes it with `crossterm::clipboard::CopyToClipboard::to_clipboard_from(text)`; inside tmux wrap it as `\x1bPtmux;\x1b<osc52>\x1b\\`.

- [ ] **Step 3: Panic restore**

Bind F12 to `panic!("poc")`. Run, press F12.
Expected: the shell prompt is usable afterwards: typing works, `Ctrl-Q` and arrow keys behave normally, no leftover `CSI u` sequences (check with `cat -v` then typing).

- [ ] **Step 4: Run the matrix**

Rows as in Task 6 step 5. For each record: keyboard protocol on or off; whether Ctrl-Q, Ctrl-C, Ctrl-V (bracketed paste), Ctrl-Enter, Alt-Enter, Ctrl-I vs Tab, Esc and Shift-Enter are distinguishable; whether OSC 52 reached the local clipboard (`wl-paste` or `xclip -o` on the local machine).

- [ ] **Step 5: Legacy Ctrl-Q**

Run in a terminal without the kitty keyboard protocol with `stty ixon` set beforehand.
Expected: Ctrl-Q arrives as a key (raw mode clears IXON). Record the result.

- [ ] **Step 6: Findings**

Write `pawsman/docs/research/keyboard-and-clipboard.md`: the matrix, the key bindings that need legacy alternatives, OSC 52 behaviour per row (kitty asks before clipboard reads; record what writes do), and the mode list for `term::MODES`. Commit `docs: keyboard protocol and OSC 52 findings`. Close the issue.

---

### Task 8: PoC 3, Rhai sandbox limits

**Files:**
- Create: `~/src/github/pawsman-poc/rhai-limits/Cargo.toml`, `rhai-limits/src/lib.rs`
- Create (pawsman): `docs/research/rhai-sandbox.md`

**Interfaces:**
- Produces (design input): `sandbox(budget: Duration) -> rhai::Engine`.

- [ ] **Step 1: Write the limit tests**

```rust
//! Sandboxed Rhai engine: every script either finishes or fails within its budget.

use std::time::{Duration, Instant};

pub fn sandbox(_budget: Duration) -> rhai::Engine {
    rhai::Engine::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(script: &str) -> (Result<rhai::Dynamic, Box<rhai::EvalAltResult>>, Duration) {
        let engine = sandbox(Duration::from_millis(200));
        let start = Instant::now();
        let out = engine.eval::<rhai::Dynamic>(script);
        (out, start.elapsed())
    }

    #[test]
    fn infinite_loop_stops_within_budget() {
        let (out, took) = run("loop {}");

        assert!(out.is_err(), "loop must be stopped");
        assert!(took < Duration::from_millis(400), "stopped in {took:?}");
    }

    #[test]
    fn huge_string_is_refused() {
        let (out, _) = run(r#"let s = "x"; loop { s += s; }"#);

        assert!(matches!(*out.unwrap_err(), rhai::EvalAltResult::ErrorDataTooLarge(..)), "string cap");
    }

    #[test]
    fn huge_array_is_refused() {
        let (out, _) = run("let a = []; loop { a.push(1); }");

        assert!(out.is_err(), "array cap or budget");
    }

    #[test]
    fn deep_recursion_is_refused() {
        let (out, _) = run("fn f(n) { f(n + 1) } f(0)");

        assert!(matches!(*out.unwrap_err(), rhai::EvalAltResult::ErrorStackOverflow(..)), "call depth");
    }

    #[test]
    fn no_file_or_eval_access() {
        let (out, _) = run(r#"eval("1 + 1")"#);

        assert!(out.is_err(), "eval disabled");
    }

    #[test]
    fn normal_script_runs() {
        let (out, _) = run("let x = 40; x + 2");

        assert_eq!(out.unwrap().as_int().unwrap(), 42);
    }
}
```

- [ ] **Step 2: Run to fail, then implement**

Run: `cargo test -p rhai-limits` → the first five fail or hang (run with `timeout 30`).
Implement `sandbox`:

```rust
pub fn sandbox(budget: Duration) -> rhai::Engine {
    let mut engine = rhai::Engine::new();
    engine.set_max_operations(5_000_000);
    engine.set_max_call_levels(64);
    engine.set_max_expr_depths(64, 32);
    engine.set_max_string_size(1 << 20);
    engine.set_max_array_size(100_000);
    engine.set_max_map_size(100_000);
    engine.disable_symbol("eval");
    let start = Instant::now();
    engine.on_progress(move |_| (start.elapsed() > budget).then(|| "time budget exceeded".into()));
    engine
}
```

Run again → 6 pass. Note: `start` is captured at engine creation; record in the findings that M2 must reset the clock per run (an engine per run, or a shared `Cell<Instant>` set before each `eval`).

- [ ] **Step 3: Measure**

Time `Engine` creation and a 1 000-line script over 1 000 runs (`std::time::Instant`, release build). Record both.

- [ ] **Step 4: Findings**

Write `pawsman/docs/research/rhai-sandbox.md`: the limits and why, per-run clock reset, cost per engine, which std packages stay registered, and the planned `req`/`res`/`vars` API surface for M2. Commit `docs: rhai sandbox findings`. Close the issue.

---

### Task 9: PoC 4, tachyonfx splash

**Files:**
- Create: `~/src/github/pawsman-poc/splash/Cargo.toml`, `splash/src/main.rs`, `splash/src/splash.rs`
- Create (pawsman): append findings to `docs/research/startup-animation.md`

**Interfaces:**
- Produces (design input for M1's splash): `splash::Splash::new(seed: u64) -> Splash`, `Splash::frame(&mut self, elapsed: Duration, buf: &mut Buffer, area: Rect)`, `Splash::skip(&mut self)`, `Splash::done(&self) -> bool`, `splash::fits(area: Rect) -> bool`.

- [ ] **Step 1: Write the tests**

`splash/src/splash.rs` with the logo as a `const LOGO: &[&str]` (a hand-drawn "pawsman" wordmark with a paw, at most 48×8 cells), and tests:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{buffer::Buffer, layout::Rect, style::Color};
    use std::time::Duration;

    fn render_at(ms: u64) -> Buffer {
        let area = Rect::new(0, 0, 80, 24);
        let mut buf = Buffer::empty(area);
        let mut splash = Splash::new(42);
        splash.frame(Duration::from_millis(ms), &mut buf, area);
        buf
    }

    #[test]
    fn same_seed_same_frame() {
        assert_eq!(render_at(600), render_at(600), "deterministic with a seed");
    }

    #[test]
    fn done_after_duration() {
        let area = Rect::new(0, 0, 80, 24);
        let mut buf = Buffer::empty(area);
        let mut splash = Splash::new(42);

        splash.frame(Duration::from_millis(1300), &mut buf, area);

        assert!(splash.done(), "finished after 1.3 s");
    }

    #[test]
    fn skip_finishes_at_once() {
        let mut splash = Splash::new(42);

        splash.skip();

        assert!(splash.done(), "skip jumps to the end");
    }

    #[test]
    fn too_small_does_not_fit() {
        assert!(!fits(Rect::new(0, 0, 20, 5)), "smaller than the logo");
    }

    #[test]
    fn indexed_mode_leaves_no_rgb() {
        let area = Rect::new(0, 0, 80, 24);
        let mut buf = Buffer::empty(area);
        let mut splash = Splash::new(42).indexed();

        splash.frame(Duration::from_millis(600), &mut buf, area);

        assert!(buf.content.iter().all(|c| !matches!(c.fg, Color::Rgb(..)) && !matches!(c.bg, Color::Rgb(..))));
    }
}
```

- [ ] **Step 2: Run to fail, then implement**

Run `cargo test -p splash` against stubs (`done` false, `fits` true) → fail. Implement:
- `Splash::new(seed)`: the research doc's effect, `parallel(&[coalesce((500, QuadOut)).with_rng(SimpleRng::new(seed)).with_filter(CellFilter::Text), sequence(&[sleep(150), fade_from_fg(BG, (600, CubicOut))]), delay(500, hsl_shift_fg([40.0, 0.0, 10.0], (500, SineInOut)))])`.
- `frame`: clear `buf`, paint `LOGO` centred, then `effect.process(elapsed - last, buf, area)` (keep `last`), and in indexed mode map each `Color::Rgb` cell to the nearest xterm-256 index.
- `skip`: `effect.process(Duration::from_secs(3600), …)` on a scratch buffer, marking done.
- `fits(area)`: `area.width >= LOGO_W + 4 && area.height >= LOGO_H + 2`.
Run → 5 pass.

- [ ] **Step 3: Live loop**

`splash/src/main.rs`: after setup, while `!splash.done()`: draw, `event::poll(16ms)`; any key → `skip()`; Ctrl-Q → quit; then show "main view" text and block on `event::read`. Run in rows 1, 2, 3 and 6 of Task 6's matrix; record smoothness and bytes written (`strace` as before).

- [ ] **Step 4: Visual check**

Render frames at 0, 300, 600, 900, 1200 ms to PNGs with the visual-check method (TestBackend buffer to PNG, one pixel block per cell). Show them to the user and get approval of the look before M1 builds the real splash.

- [ ] **Step 5: Findings**

Append to `pawsman/docs/research/startup-animation.md`: measured bytes per frame locally and over SSH, frame rate, the final effect parameters the user approved. Commit `docs: splash prototype findings`. Close the issue.

---

### Task 10: Close M0

- [ ] **Step 1: Decisions**

Add one `docs/DECISIONS.md` bullet per PoC outcome (graphics implementation choice, probe timeout, keyboard fallbacks, Rhai limits, splash parameters), each linking its research doc.

- [ ] **Step 2: Gates**

Run: `cargo nextest run --workspace && CARGO_BUILD_WARNINGS=deny cargo clippy --workspace --all-targets --locked && tools/check-deps.sh && markdownlint-cli2 "**/*.md" && typos`
Expected: all clean.

- [ ] **Step 3: PR and close**

Open a PR with the research docs and decisions (`Closes` each remaining M0 issue). After it merges: `gh api -X PATCH repos/sm-steel/pawsman/milestones/<M0 number> -f state=closed`.

- [ ] **Step 4: Next**

Start M1 with brainstorming on its scope (the splash first), using the findings as inputs.
