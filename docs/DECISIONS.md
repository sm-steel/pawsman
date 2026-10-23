# Decisions

One line per non-obvious decision.

- **Inspired remake, own format** — Bruno files are an import source only, so the format can be designed for TOML and git diffs (→ [spec](superpowers/specs/2026-10-23-pawsman-design.md#what-the-user-chose)).
- **Declarative captures and assertions, Rhai as escape hatch** — no JavaScript; most chaining needs no code, Rhai is pure Rust and sandboxable (→ [spec §2](superpowers/specs/2026-10-23-pawsman-design.md#request-file)).
- **HTTP, GraphQL, WebSocket, gRPC, SSE** — Bruno's protocol set (→ [inventory](re/bruno-feature-inventory.md)).
- **Kitty-first, degrade by probe** — views branch on probed `Caps`, never on `$TERM` (→ [rules/tui.md §9](../rules/tui.md)).
- **TOML, one request per file** — readable diffs, comments, serde support (→ [spec §2](superpowers/specs/2026-10-23-pawsman-design.md#2-on-disk-format-variables-and-secrets)).
- **Layered secrets** — env and `.env`, then the OS keyring, then an age vault, so headless SSH sessions still work (→ [spec §2](superpowers/specs/2026-10-23-pawsman-design.md#secrets)).
- **Sync main loop, engine thread, private tokio thread** — one terminal owner, networking isolated on its own runtime (→ [ARCHITECTURE §2](ARCHITECTURE.md#2-threads)).
- **Ctrl-Q quits** — Ctrl-C and Ctrl-V stay copy and paste (→ [rules/tui.md §5](../rules/tui.md)).
- **Cell-based tachyonfx splash** — kitty frame animation costs too much over SSH (→ [startup animation](research/startup-animation.md)).
- **typos learns `ratatui`** — its dictionary corrects the crate name to "ratatouille" on every mention (`_typos.toml`).
- **The agent self-approves** — tests, snapshots, visuals and dependencies are reviewed and approved by the agent, with reasons recorded (→ [rules §1](../rules/rust-project.md#1-working-agreement)).
