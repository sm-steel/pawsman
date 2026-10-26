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
- **Dev tools straight from GitHub when the mise mirror lacks them** — a one-off `MISE_URL_REPLACEMENTS='{}' mise install`; the shared mise config is never edited.
- **Public repository** — GitHub Actions would not run for the private repo on this account; the full history was scanned with gitleaks before the switch.
- **F5 sends in every terminal** — Ctrl-Enter is plain Enter in tmux and legacy terminals, and xterm drops Alt-Enter (→ [keyboard and clipboard](research/keyboard-and-clipboard.md#over-ssh-in-tmux-and-in-xterm)).
- **Images Off in a non-UTF-8 tmux** — tmux rewrites kitty placeholders and half blocks to `_` without UTF-8 (→ [kitty graphics](research/kitty-graphics-over-ssh.md#matrix)).
- **Terminal matrix in Docker** — sshd, tmux and xterm in a throw-away container reached over SSH, so nothing on the desktop changes (`pawsman-poc/matrix`).
- **The agent self-approves** — tests, snapshots, visuals, dependencies, merges of its own PRs and minor review findings are decided by the agent, with reasons recorded (→ [rules §1](../rules/rust-project.md#1-working-agreement)).
- **Own kitty encoder, not ratatui-image's** — ratatui-image 11.1.0 sends uncompressed RGBA by default, has no PNG path, and enables its opt-in zlib only through its own probe; pawsman sends PNG or zlib RGBA, whichever is smaller, through its single probe (→ [kitty graphics](research/kitty-graphics-over-ssh.md#decision-for-m1)).
- **Probe timeout 1 s plus 150 ms grace** — a silent terminal costs 1.15 s once and leaks no reply bytes (→ [kitty graphics](research/kitty-graphics-over-ssh.md#probe)).
- **Button-event mouse tracking only** — crossterm's `EnableMouseCapture` reports every motion, which would wake the loop (→ [keyboard and clipboard](research/keyboard-and-clipboard.md#mouse)).
- **Alt-Enter as the legacy send key** — Ctrl-Enter is plain Enter without the kitty keyboard protocol (→ [keyboard and clipboard](research/keyboard-and-clipboard.md#keys)).
- **Rhai limits and an engine per run** — operations, depth, sizes and a per-run time budget; an engine costs 0.5 ms (→ [Rhai sandbox](research/rhai-sandbox.md)).
- **Unbounded-memory tests run under a memory cap** — an unlimited Rhai engine exhausted the machine once (→ [Rhai sandbox](research/rhai-sandbox.md#limits)).
- **Splash effects tick on a scratch buffer and stay inside the logo** — tachyonfx stalls on long steps and compounds repeated ticks; whole-screen effects cost 20 KB per frame (→ [startup animation](research/startup-animation.md#prototype-findings)).
- **M1 in seven pieces, splash first** — the binary and splash ship before any API-client feature, then the thinnest path from file to response, then each part widens (→ [M1 spec](superpowers/specs/2026-10-26-m1-http-client-design.md#pieces)).
- **Own input decoder over `/dev/tty`** — crossterm reads the terminal itself and would type a late probe reply as keys (→ [M1 spec](superpowers/specs/2026-10-26-m1-http-client-design.md#cross-cutting-decisions)).
- **Own HTTP stack on hyper** — reqwest cannot time connect and TLS separately for the timeline (→ [M1 spec](superpowers/specs/2026-10-26-m1-http-client-design.md#cross-cutting-decisions)).
- **XDG paths from the env snapshot on Linux and macOS** — one layout, no directories crate (→ [M1 spec](superpowers/specs/2026-10-26-m1-http-client-design.md#cross-cutting-decisions)).
- **tmux UTF-8 judged from the locale** — pawsman cannot spawn `tmux` to ask; `--images` overrides (→ [M1 spec](superpowers/specs/2026-10-26-m1-http-client-design.md#cross-cutting-decisions)).
- **typos learns `flate`** — it splits the crate name `flate2` and corrects the stem (`_typos.toml`).
