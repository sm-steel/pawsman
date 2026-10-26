# Testing

| Kind | Tool | Where |
| --- | --- | --- |
| Unit | `#[cfg(test)] mod tests` as the file's last item | every crate |
| Integration | one binary per crate, `tests/it/main.rs` | crates with I/O |
| Snapshots | insta; format round trips, rendered `TestBackend` buffers | model, tui |
| Properties | proptest; interpolation, TOML round trips, cURL parser | model, import |
| Tables | rstest named cases | everywhere |
| Fuzzing | cargo-fuzz; cURL, Postman, Bruno parsers | import |
| Mutation | `cargo mutants --in-diff` before opening a PR | locally |
| Visual | PNG files from `TestBackend` (`visual-check` skill) | tui |
| Terminal | PTY tests replaying canned replies for each probe and fallback (pty-process plus a vt100 screen) | cli (`tests/it`) |

- Tests first: a `test:` commit failing on assertions, then the implementation ([rules §1](../rules/rust-project.md#1-working-agreement)).
- Test names are sentences; arrange, act, assert separated by blank lines; assertions carry a message.
- Fakes, not mocks: wiremock for HTTP, fake WebSocket, SSE and gRPC servers in `pawsman-test-support`.
- Fake secrets contain `PAWSMAN_FAKE`; gitleaks allowlists only that marker.
- Async tests use `#[tokio::test(start_paused = true)]`.
- Runner: cargo-nextest (`.config/nextest.toml`); CI sets `NEXTEST_PROFILE=ci` for no fail-fast and JUnit output.
