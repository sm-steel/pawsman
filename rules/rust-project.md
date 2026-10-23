# Rust project rules

Conventions extracted from semoxide; they win every conflict with [tui.md](tui.md) and [rust-api-client.md](rust-api-client.md).

## 1. Working agreement

- Architecture and functionality follow the approved [program spec](../docs/superpowers/specs/2026-10-23-pawsman-design.md) and each milestone's spec. Decisions inside that frame are the agent's; each one gets a line in [DECISIONS.md](../docs/DECISIONS.md). Scope beyond the spec goes to the user.
- Research, docs and PoCs come before product code; PoCs live outside this repo (`pawsman-poc`).
- Tests first: write the tests, show them failing **on assertions** (not `todo!()` or compile errors), commit as `test: …`, then implement in a separate `feat:`/`fix:` commit.
- **The agent reviews and approves its own work.** Test approval, snapshot review, visual checks, dependency choices, merging its own PRs once the gates pass, and fixing or ruling on minor review findings are the agent's, never a stop for the user. Each approval is recorded with its reason: in the plan ledger, the commit body or the PR description.
- A test, once approved, is never weakened, skipped, deleted or rewritten to make code pass. A change to an existing test goes in its own `test:` commit, and the PR lists it with the reason.
- Snapshots: read every `.snap.new` diff before accepting it with `cargo insta accept`; the PR lists accepted snapshots.
- Never loosen a lint, threshold, exclusion or allowlist to silence a true finding; fix the code. A genuine false positive gets the narrowest possible allowance (one word, one site, `#[expect(…, reason = "…")]`) with a comment saying why.
- Dependencies: pinned with `=`; licence in the `deny.toml` allow-list; maintained (a release within the last year or an obviously finished crate); not duplicating what std or an existing dependency does. The PR names each new dependency and why.
- Work on a branch, never `main`; a PR closes its issue; PRs are small.
- Conventional Commits; Renovate uses `chore(deps)`. Never `--no-verify`. Never override the git commit email.
- Every task has a short spec: scope, out of scope, failure table (what can fail, error code, retryable), an end-to-end check.
- Rules that must always hold live in the compiler, clippy, CI or hooks, not in prose.
- GitHub milestones and issues are the agent's to create, update and close; a milestone never holds a single issue.

## 2. Rust code rules

1. No `unwrap` / `expect` / `panic!` / `todo!` / `unimplemented!` / indexing (`v[i]`) outside tests ([RUST-PITFALLS](../docs/RUST-PITFALLS.md)). Enforced by clippy, relaxed in tests by `clippy.toml`.
2. `#[expect(lint, reason = "…")]`, never `#[allow]` (`allow_attributes`, `allow_attributes_without_reason`).
3. Meaning in types: newtypes and enums, not `bool`/`Option`/`String` parameters (`max-fn-params-bools = 0`). E.g. `RequestId`, `EnvName`, `VarName`.
4. Struct fields are private; construct with `new` or a builder.
5. Conversions via `From`/`TryFrom`/`FromStr` (`fallible_impl_from`); naming `as_`/`to_`/`into_`; no `get_` prefix.
6. No `unsafe`: workspace `unsafe_code = "deny"` and `#![forbid(unsafe_code)]` in every crate root.
7. Modern std over extra crates: let chains, `if let` guards, `LazyLock`/`OnceLock`, `cfg_select!`, `assert_matches!`.
8. Flat control flow: early returns and `?` (`excessive-nesting-threshold = 5`).

- `//` comments are short, or absent when the code speaks. `///` on every public item (`missing_docs`), short. Comments explain the code, never its history.
- No iteration over `HashMap`/`HashSet` (`iter_over_hash_type`); use `BTreeMap`/`IndexMap` or sort.
- `#[must_use]` and `debug_assert!` for invariants types can't express.
- `#[non_exhaustive]` on public enums and error structs.
- Concrete types until a second implementation exists; traits only at real seams.

## 3. Workspace layout

- Flat `crates/*` workspace with `[workspace.package]` inheritance; MSRV is the pinned toolchain minus 2, checked by `cargo hack --rust-version`.
- Crates split by **purity and heavy dependencies**, not by size ([CODE-ARCHITECTURE §1](../docs/CODE-ARCHITECTURE.md)). No tiny crates.
- Engine first: the TUI and the CLI are thin over `pawsman-core`.
- Every dependency declared once in `[workspace.dependencies]`, pinned `"=x.y.z"`; crates use `dep.workspace = true` and `[lints] workspace = true`.
- Crate boundaries enforced by `deny.toml` `bans.deny` wrappers and `tools/check-deps.sh`.
- `pawsman-test-support` (`publish = false`) holds fakes and fixtures, used as a path dev-dependency.
- Features: few, additive, `dep:` syntax.

### Modules and files

- One domain or concept per file; a type and the code that builds and validates it live together (parse, don't validate).
- Split by domain, not by stage (no `types.rs` + `parsing.rs`); never by size alone.
- `foo.rs` + `foo/child.rs`, never `mod.rs`. The parent holds the tying type and `pub use` re-exports; children share via `pub(super)`. Components inside a crate are `pub(crate)`.

## 4. Errors

- Each crate has its own error type: a public `#[non_exhaustive]` struct over a private `Kind` enum, `pub(crate)` constructors, hand-written `Display` and `std::error::Error`. No `anyhow`, no `thiserror`.
- Every error implements `pawsman_error::ErrorInfo`: `code()` (namespaced constants in the crate's `codes.rs`, listed in `codes::ALL`), `help()`, `retry()`.
- Render at the edge only: TUI notices and `pawsman run` reports.
- Wording per [docs/errors/README.md](../docs/errors/README.md). A registry test fails on a code missing from `ALL` or without a docs page.

## 5. Async and concurrency

- Async only in `pawsman-net` and `pawsman-grpc`, on a private tokio runtime thread; no public `async fn` or tokio types leak out ([CODE-ARCHITECTURE §3](../docs/CODE-ARCHITECTURE.md)).
- No work outlives its owner: cancellation tokens, joined threads on shutdown.
- `std::thread::scope` for sync parallel read-only work.
- Async tests use `#[tokio::test(start_paused = true)]`.

## 6. Logging and secrets

- Libraries emit `tracing` events only; only the binary installs the subscriber, writing to a file ([OBSERVABILITY](../docs/OBSERVABILITY.md)).
- Every step and every remote operation emits a `tracing` event.
- Env vars only through the env snapshot (`clippy.toml` bans `std::env::var*`); never `std::env::temp_dir` (use `tempfile`).
- Secrets are `secrecy` types; never format or log `expose_secret()`.
- URLs are logged only through `url_for_log()`.
- `PAWSMAN_LOG` overrides, falling back to `RUST_LOG`; default `pawsman=info`, dependencies `warn`.

## 7. Testing

See [TESTING.md](../docs/TESTING.md): inline `mod tests` as the last item, `tests/it/main.rs` per crate, insta, rstest, proptest, fuzzing for parsers, fakes not mocks, the `PAWSMAN_FAKE` marker, nextest, `cargo mutants --in-diff`.

## 8. Lints and tooling

- Lints: the `[workspace.lints]` tables in `Cargo.toml` and the root `clippy.toml`; warnings become errors through `CARGO_BUILD_WARNINGS=deny` in hooks and CI.
- Dev tools pinned in `mise.toml`; Rust in `rust-toolchain.toml`; hooks in `lefthook.yml`; CI in `.github/workflows/`.

## 9. Docs

- Short, technical, never repeated (link instead); each index is one line per entry.
- Mermaid for flows, sequences and architecture; never ASCII art.
- `CLAUDE.md` holds the rules summary and doc index; `AGENTS.md` points agents to it.
- Skills in `.claude/skills/` for recurring work (`rust-testing`, `rust-async`, `visual-check`).
