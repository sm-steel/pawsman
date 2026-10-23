# AGENTS.md

Instructions for AI coding agents (and people) working on pawsman.

## Start here

- Read [CLAUDE.md](CLAUDE.md): project rules, Rust rules, tools and the doc index. It applies to every agent, not only Claude.
- Read the skills in `.claude/skills/` before the matching work: `rust-testing` (any test), `rust-async` (any concurrency), `visual-check` (any UI change).
- Setup per clone: [README § Development](README.md#development).

## Workflow

- Work on a branch, never on `main`; a pull request closes its issue.
- Commit messages follow [Conventional Commits](https://www.conventionalcommits.org/).
- The git hooks must pass; never commit or push with `--no-verify`.
- Tests follow the `rust-testing` skill: tests first, failing on assertions, reviewed and approved by the agent before implementing ([rules/rust-project.md §1](rules/rust-project.md#1-working-agreement)).
- The PR description lists every change to an existing test, every accepted snapshot and every new dependency.

## Never

- Loosen a lint, threshold, exclusion or allowlist to hide a true finding: fix the code. A genuine false positive gets the narrowest allowance, with a comment saying why.
- Accept a snapshot (`.snap.new`) without reading its diff.
- Weaken, skip, delete or rewrite an approved test to make code pass; a change to a test is its own `test:` commit with its reason.
