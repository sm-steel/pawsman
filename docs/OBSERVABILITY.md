# Observability

How pawsman reads its environment and logs.

| # | Rule |
| --- | --- |
| P1 | The process env is read once, into an env snapshot; nothing else calls `std::env::var*` (`clippy.toml`) |
| P2 | Libraries emit `tracing` events only; the binary installs the subscriber |
| P3 | The log is a daily rolling file under `~/.local/state/pawsman/`, never the terminal |
| P4 | `PAWSMAN_LOG` sets the filter, falling back to `RUST_LOG`; default `pawsman=info`, dependencies `warn` |
| P5 | URLs are logged only through `url_for_log()`, which drops userinfo and secret query values |
| P6 | Secrets are `secrecy` types and never logged; request and response bodies are never logged |
| P7 | Every request logs `method`, `host`, `path`, `status`, `ms` |
