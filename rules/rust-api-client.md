# API client rules

Rules for request execution, storage and secrets. [rust-project.md](rust-project.md) wins every conflict.

1. **Threads.** The main loop owns the terminal; the engine thread owns `AppState`; the net thread owns the tokio runtime. They talk only through `Command`, `Request` and `Event` enums over channels ([CODE-ARCHITECTURE §3](../docs/CODE-ARCHITECTURE.md)).
2. **Requests are values.** Interpolation is a pure function from a request and its scopes to a resolved request plus a trace; the net layer only sends resolved requests.
3. **Unknown variables fail.** An unresolved `{{name}}` is `model::unknown_var`, never an empty string.
4. **Secrets never touch disk in plain text.** They are `secrecy::SecretString` from resolution to the wire; collection files hold names only; history, logs and cURL export redact them unless the user asks to include them.
5. **TLS checks stay on.** Disabling certificate checks is a per-collection setting the user sets explicitly; the danger methods are banned elsewhere (`clippy.toml`).
6. **Cancel everything.** Every in-flight request, stream and connection has a cancellation token owned by the engine; quitting cancels and joins them.
7. **Bodies are bounded.** Response bodies stream into memory up to a cap and spill to a temp file beyond it; history keeps at most the cap.
8. **Files are the source of truth.** The store writes atomically (temp file plus rename) and the watcher reloads external edits; an unparsable file shows as an error node, never a crash.
