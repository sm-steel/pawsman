# Avoid matching the waiting shell

Do not wait on or kill processes using pgrep -f/pkill -f patterns that also appear in the same shell command line: the waiter finds itself forever, or kills its own shell. This left two M1.3 waiters running for days.

Prefer the Codex process session/completion mechanism for work you started. If process discovery is necessary, use an exact process name (`pgrep -x`), verify `/proc/<pid>/comm` and ownership, then operate on a checked PID. Do not reuse old transcript PIDs or Claude-specific background-tool names.

Source: [Claude memory](/home/smsteel/.claude/projects/-home-smsteel-src-github-pawsman/memory/pgrep-self-match.md). Migrated 2026-10-30; scope: this project; status: active.
