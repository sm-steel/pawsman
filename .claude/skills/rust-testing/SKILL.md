---
name: rust-testing
description: Use when writing, changing or fixing tests in pawsman, implementing pure logic (format, interpolation, scopes, captures, assertions, parsers, importers), or when a snapshot (insta) changes or a test fails
---

# Rust testing in pawsman

## Overview

Tests are the spec. The agent writes them first, reviews them, approves them and records why ([rules §1](../../../rules/rust-project.md#1-working-agreement)). How much ceremony depends on the area:

| Area | Approach |
| --- | --- |
| Pure logic: model, format, interpolation, scopes, captures, assertions, cURL and importer parsers, probe reply parser | the strict sequence below |
| Views and rendering | code and tests together; insta snapshots of `TestBackend` buffers, reviewed before accepting; visual PNG files read (`visual-check`) |
| CLI (`pawsman run`) | an `assert_cmd` case first, then the code |
| Net, store, watcher, terminal I/O | characterization tests against fakes after a PoC; every bug gets a failing reproduction test first |

## Pure logic: the sequence

1. **Tests only**, from the spec's tables and rules, in the `#[cfg(test)] mod tests { … }` of the module under test.
2. **Real red.** The stub returns a wrong but valid value, so the tests fail **on their assertions**. A `todo!()` panic or a compile error doesn't count. A test that already passes against the stub stays and is noted as "passes on stub".
3. **One `test: …` commit** with the tests and the stub.
4. **Review and approve.** Check each test names a real requirement and fails for the missing behaviour; record `self-approved tests — <why>` in the ledger or the commit body.
5. Implement in a separate `feat:`/`fix:` commit. Never weaken, skip, delete or rewrite an approved test to make code pass; a test that turns out wrong changes in its own `test:` commit with the reason.

New dependencies, including dev-dependencies, follow the dependency rule in [rules §1](../../../rules/rust-project.md#1-working-agreement).

## Snapshots (insta)

Run `cargo insta test`, then read every `.snap.new` diff (`cargo insta review` or the file itself) before `cargo insta accept`. Accept only diffs that are exactly the intended change; list accepted snapshots in the PR. Never `INSTA_UPDATE=always`, never hand-edit `.snap` files.

| Excuse | Reality |
| --- | --- |
| "It's a trivial reorder, accept all" | Read it anyway: reorders hide dropped lines. |
| "CI must be green" | Green from an unread snapshot is fake green. |

## Placement

Unit tests inline as the file's last item; no out-of-line `mod tests;`. Integration tests in `tests/it/`. Fakes and fixtures in `pawsman-test-support`; fake secrets contain `PAWSMAN_FAKE`.
