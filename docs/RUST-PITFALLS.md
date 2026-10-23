# Rust pitfalls

Known Rust problems that apply to pawsman, and how we avoid them. Rows 1–16 come from semoxide's research of large Rust CLIs.

| # | Pitfall | Mitigation | Where |
| --- | --- | --- | --- |
| 1 | Slow incremental rebuilds in workspaces | split hot crates, trim deps and features, dev deps at `opt-level=3` | [CODE-ARCH §1](CODE-ARCHITECTURE.md#1-crate-split) |
| 2 | Crate splits help and hurt | split only on real boundaries; no tiny crates | [CODE-ARCH §1](CODE-ARCHITECTURE.md#1-crate-split) |
| 3 | Proc-macro and serde build cost | serde types live in `pawsman-model` | [CODE-ARCH §1](CODE-ARCHITECTURE.md#1-crate-split) |
| 4 | Async cancellation stops work mid-operation | async only on the net thread; cancellation tokens owned by the engine | [CODE-ARCH §3](CODE-ARCHITECTURE.md#3-async) |
| 5 | Non-deterministic async tests | `#[tokio::test(start_paused = true)]` | [TESTING](TESTING.md) |
| 6 | Errors shaped for display, not for code | typed errors + `ErrorInfo`; rendering at the edge | [CODE-ARCH §4](CODE-ARCHITECTURE.md#4-errors) |
| 7 | Over-generic, trait-heavy code | concrete types until a second implementation exists | [rules §2](../rules/rust-project.md#2-rust-code-rules) |
| 8 | Orphan rule: no foreign trait on a foreign type | newtype + extension trait | – |
| 9 | Dependency bloat | audit each dependency; cargo-deny | [rules §1](../rules/rust-project.md#1-working-agreement) |
| 10 | Supply-chain attacks | cargo-deny, `--locked`, care with new or typo-like crates | `deny.toml` |
| 11 | Panics in production | no `unwrap`; panic hook restores the terminal | [rules/tui.md §2](../rules/tui.md) |
| 12 | Feature leaks | `cargo hack --each-feature` | CI |
| 13 | Lint debt from a loose start | strict lints from day one | `Cargo.toml` |
| 14 | Rust `regex` `\d` matches non-ASCII digits | write `[0-9]` in patterns | – |
| 15 | Terminal left in a raw or kitty keyboard mode after a crash | every mode in `term::MODES` with its undo, run from the panic hook | [rules/tui.md §2](../rules/tui.md) |
| 16 | Counting `chars()` for layout breaks wide characters | measure graphemes with unicode-width | [rules/tui.md §6](../rules/tui.md) |
