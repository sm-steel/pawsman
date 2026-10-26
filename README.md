# pawsman

A terminal API client inspired by [Bruno](https://github.com/usebruno/bruno): offline, file-based, git-friendly collections in a fast TUI. Pure Rust, kitty graphics and keyboard protocols first, and it works the same over SSH.

> **Status:** M1 in progress. The binary starts, probes the terminal and shows the startup animation; collections and requests come next.

## Run

```sh
cargo install --path crates/pawsman-cli
pawsman                 # Ctrl-Q quits, F1 lists the keys
pawsman --no-splash     # skip the startup animation (also `PAWSMAN_NO_ANIMATION=1`)
pawsman --images blocks # force an image mode: auto, kitty, sixel, blocks or off
```

Settings live in `~/.config/pawsman/settings.toml` (`[ui] animations`, `images`); the log goes to `~/.local/state/pawsman/` (`PAWSMAN_LOG=pawsman=debug` for more). Inside tmux, run tmux with a UTF-8 locale (`tmux -u`) and `set -g allow-passthrough on` for kitty images.

## Development

1. Install [rustup](https://rustup.rs) and [mise](https://mise.jdx.dev).
2. `mise install` installs the pinned dev tools; `lefthook install` sets up the git hooks.
3. Read [AGENTS.md](AGENTS.md) before contributing.

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or [MIT license](LICENSE-MIT) at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in pawsman by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.
