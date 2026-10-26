# pawsman

A pure-Rust terminal API client inspired by Bruno. Current phase: **M1 HTTP client** ([spec](docs/superpowers/specs/2026-10-26-m1-http-client-design.md)), piece 1.2 ([plan](docs/superpowers/plans/2026-10-26-m1-2-thin-path.md)).

## IMPORTANT

- Architecture and functionality follow the [program spec](docs/superpowers/specs/2026-10-23-pawsman-design.md); decisions inside it are the agent's and get a line in [DECISIONS](docs/DECISIONS.md). Scope beyond the spec goes to the user.
- The agent reviews and approves its own work: tests, snapshots, visual checks, dependencies, genuine tool false positives. Each approval is recorded with its reason ([rules/rust-project.md §1](rules/rust-project.md#1-working-agreement)).
- GitHub milestones and issues are the agent's to create, update and close without asking; a milestone never holds a single issue.
- Inline comments (`//`) are short and informative; none when the code speaks. Doc comments (`///`) on every public item, short.
- Comments and docs explain the code or the rule, never their history. Provenance belongs in commits and PRs.
- One domain per file; split by domain, not by stage or size ([CODE-ARCHITECTURE](docs/CODE-ARCHITECTURE.md#modules-and-files)).
- Every step and remote operation emits `tracing` events ([OBSERVABILITY](docs/OBSERVABILITY.md)).

## Hard requirements

- Rust only: no JavaScript, no wasm, anywhere.
- Kitty protocols are first-class (graphics through Unicode placeholders, keyboard protocol, OSC 52); other terminals degrade by probed capability.
- Everything works over SSH and inside tmux.
- Engine first: the TUI and the CLI are thin over `pawsman-core`.
- Research, docs and PoCs before product code; PoCs live in `pawsman-poc`, never here.
- Docs: short, technical, never repeated (link instead). Each index is one line per entry. **Mermaid diagrams wherever they help; never ASCII art.**
- Git commit email is `sm.steel@hotmail.com` (repo-local config). Never override it.
- Tests first, failing on assertions, approved by the agent before implementation; approved tests are never weakened to make code pass.

## Rules

| File | Scope |
| --- | --- |
| [rules/rust-project.md](rules/rust-project.md) | Working agreement, Rust rules, layout, errors, async, logging, testing. Wins every conflict |
| [rules/tui.md](rules/tui.md) | Terminal ownership, modes, keys (Ctrl-Q quits), clipboard, images, splash |
| [rules/rust-api-client.md](rules/rust-api-client.md) | Threads, requests, variables, secrets, TLS, cancellation, storage |

Rust rules, in short:

1. No `unwrap` / `expect` / `panic!` outside tests.
2. `#[expect(lint, reason = "…")]`, never `#[allow]`.
3. Meaning lives in types: newtypes and enums, not `bool` / `Option` / `String` parameters.
4. Struct fields are private; construct with `new` or a builder.
5. Conversions via `From` / `TryFrom` / `FromStr`; names use `as_` / `to_` / `into_`; getters have no `get_`.
6. No `unsafe`: `#![forbid(unsafe_code)]` in every crate root.
7. Modern std: let chains, `if let` guards, `LazyLock`/`OnceLock`, `cfg_select!`, `assert_matches!`.
8. Flat control flow: early returns and `?`.

Verifying UI changes: use the `visual-check` skill.

Gates: `CARGO_BUILD_WARNINGS=deny cargo clippy --workspace --all-targets --locked`, `cargo nextest run --workspace`, `tools/check-deps.sh`, `markdownlint-cli2 "**/*.md" "#target" "#node_modules" "#.superpowers"`, `typos`.

## Tools

Pinned in `mise.toml`; Rust in `rust-toolchain.toml`. Added with the milestone that first needs them: the insta CLI (M1), cargo-fuzz (M5, nightly). rea is the user's global tool.

- Tests: [cargo-nextest](https://nexte.st/) · [insta](https://insta.rs/) · [proptest](https://proptest-rs.github.io/proptest/) · [cargo-fuzz](https://rust-fuzz.github.io/book/) · [cargo-mutants](https://mutants.rs/)
- Quality: [cargo-hack](https://github.com/taiki-e/cargo-hack) · [cargo-deny](https://embarkstudios.github.io/cargo-deny/) · [typos](https://github.com/crate-ci/typos) · [gitleaks](https://github.com/gitleaks/gitleaks) · [actionlint](https://github.com/rhysd/actionlint) · [zizmor](https://docs.zizmor.sh/) · [markdownlint-cli2](https://github.com/DavidAnson/markdownlint-cli2) · [lefthook](https://lefthook.dev/)
- Reverse engineering: [rea](https://www.npmjs.com/package/rea-agents) (Bruno analysis)

## Index

### **IMPORTANT**: every agent or subagent MUST read what the task needs

- [Program spec](docs/superpowers/specs/2026-10-23-pawsman-design.md): goal, crates, format, TUI, milestones
- [M1 spec](docs/superpowers/specs/2026-10-26-m1-http-client-design.md): current milestone, its pieces and decisions
- [M1.2 plan](docs/superpowers/plans/2026-10-26-m1-2-thin-path.md): thin path from a request file to a response
- [M1.1 plan](docs/superpowers/plans/2026-10-26-m1-1-binary-terminal-splash.md): binary, terminal layer and splash tasks
- [M0 plan](docs/superpowers/plans/2026-10-23-m0-foundations.md): finished foundations milestone
- [Architecture](docs/ARCHITECTURE.md): crates, threads, collection layout, variable scopes
- [Code architecture](docs/CODE-ARCHITECTURE.md): crate split, boundaries, async, errors, modules
- [Observability](docs/OBSERVABILITY.md): env snapshot, logging, URL redaction
- [Testing](docs/TESTING.md): test strategy, fakes, CI
- [Decisions](docs/DECISIONS.md): why each non-obvious decision was made
- [Rust pitfalls](docs/RUST-PITFALLS.md): known Rust problems and mitigations
- [Error pages](docs/errors/README.md): message wording and page template
- [Bruno feature inventory](docs/re/bruno-feature-inventory.md): Bruno's backend features from rea, mapped to milestones
- [Startup animation](docs/research/startup-animation.md): splash technique, crates and prototype findings
- [Kitty graphics over SSH](docs/research/kitty-graphics-over-ssh.md): probe, image transmission, open matrix rows
- [Keyboard and clipboard](docs/research/keyboard-and-clipboard.md): key encodings, mode stack, mouse, OSC 52
- [Rhai sandbox](docs/research/rhai-sandbox.md): limits, cost, M2 script API
- PoCs: `~/src/github/pawsman-poc` (never in this repo)
