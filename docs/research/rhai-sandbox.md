# Rhai sandbox

What the M0 PoC (`pawsman-poc/rhai-limits`, rhai 1.27.0) showed about running user scripts safely. Feeds M2's `pawsman-script` (issue #22).

## Limits

| Limit | Value | Stops |
| --- | --- | --- |
| `set_max_operations` | 5 000 000 | runaway loops that stay cheap per step |
| `on_progress` time budget | per run (200 ms in tests) | any script past its wall-clock budget, checked on every operation |
| `set_max_call_levels` | 64 | deep recursion (`ErrorStackOverflow`) |
| `set_max_expr_depths` | 64 / 32 (global / function) | deeply nested expressions at parse time |
| `set_max_string_size` | 1 MiB | string doubling (`ErrorDataTooLarge`) |
| `set_max_array_size`, `set_max_map_size` | 100 000 | unbounded growth |
| `disable_symbol("eval")` | – | building code from strings |

With every limit set, the six PoC tests (infinite loop within budget, huge string, huge array, deep recursion, `eval` refused, a normal script returns its value) pass in 0.22 s together.

**An engine without limits can exhaust the machine's memory**: string doubling and array growth ran until the OS killed the process. Tests that exercise unbounded scripts run inside a memory cap (`systemd-run --user --scope -p MemoryMax=1G -p MemorySwapMax=0`), never bare.

## Time budget per run

`on_progress` captures the start time when the engine is built. M2 resets it per run: one engine per run, or a shared `Cell<Instant>` set before each `eval`. One engine per request is affordable (below).

## Cost (release build)

| Step | Time |
| --- | --- |
| Build a sandboxed engine | 0.53 ms |
| Compile a 1 000-line script | 12 ms |
| Run the compiled 1 000-line script | 0.21 ms |

Compile once per script file change and cache the `AST`; build an engine per run.

## M2 API surface

- `req`: the resolved request (method, url, headers, body), read-only in `post`, writable in `pre`.
- `res`: status, headers, body as text and as parsed JSON (`res.json`), timings; only in `post`.
- `vars`: runtime variables (read and write); writes are session-only unless persisted explicitly ([spec §2](../superpowers/specs/2026-10-23-pawsman-design.md#variable-resolution)).
- `assert(cond, msg)` and `test(name, || …)` feed the Tests tab.
- No file, process, network or clock access beyond what these objects expose; the std packages stay (strings, arrays, maps, math).
