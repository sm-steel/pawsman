# Code architecture

Where code goes and the rules that keep the crates apart. Crate list: [ARCHITECTURE §1](ARCHITECTURE.md#1-crates).

## 1. Crate split

Crates split along purity and heavy dependencies, never by size. The pure crates (`pawsman-error`, `pawsman-model`, `pawsman-script`, `pawsman-import`) do no I/O; each heavy dependency has one owning crate.

## 2. Boundaries

- `deny.toml` `bans.deny` wrappers: tokio only through `pawsman-net`/`pawsman-grpc`/`pawsman-mock`, ratatui and crossterm only through `pawsman-tui`, rhai only through `pawsman-script`, tonic only through `pawsman-grpc`; no openssl, no anyhow.
- `tools/check-deps.sh` checks the normal dependency tree of each crate against what it must never link.
- Only the `$EDITOR` launcher and the browser opener (local sessions, OAuth) in `pawsman-tui` spawn processes (`clippy.toml`).

## 3. Async

Async lives only on the net runtime thread. Crates expose sync calls and channels: no public `async fn`, no tokio types in public signatures. Every task has a cancellation token owned by the engine; shutdown cancels and joins.

## 4. Errors

A public `#[non_exhaustive]` struct over a private `Kind` enum, `pub(crate)` constructors, hand-written `Display` and `std::error::Error`, and `pawsman_error::ErrorInfo`. Codes are constants in the crate's `codes.rs`, listed in `codes::ALL`; a registry test checks each has a page ([errors](errors/README.md)).

## Modules and files

- One domain per file; a type and the code that builds and validates it live together.
- Split by domain, not by stage; never by size alone.
- `foo.rs` + `foo/child.rs`, never `mod.rs`; the parent re-exports, children share via `pub(super)`.

## Pitfalls

| # | Rule | Enforced by |
| --- | --- | --- |
| P2 | Temp files through `tempfile`, never `std::env::temp_dir` | `clippy.toml` |
