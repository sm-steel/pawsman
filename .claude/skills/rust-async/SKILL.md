---
name: rust-async
description: Use when adding concurrency, timeouts, cancellation, tokio, async/await, spawned tasks, threads or channels anywhere in pawsman
---

# Async and concurrency in pawsman

## Overview

pawsman runs a few long-lived threads that talk through channels ([ARCHITECTURE §2](../../../docs/ARCHITECTURE.md#2-threads)): the main loop (sole terminal owner), input, engine, net (a private tokio runtime), image worker, clipboard, fs watcher. Async exists only on the net thread.

## Rules

| Where | Allowed |
| --- | --- |
| `pawsman-error`, `-model`, `-script`, `-import` | no tokio, no async, no threads |
| `pawsman-net`, `pawsman-grpc` | tokio on a private runtime thread; public API is sync calls plus channels |
| `pawsman-core` | the engine thread; sync; sends `Request`s to net and receives `Event`s |
| `pawsman-tui` | the main loop and its worker threads; never blocks on the engine |

A public `async fn`, or a public type exposing tokio, outside the net crates is wrong. Redesign instead.

## Timeouts and cancellation

- **No work outlives its owner.** Every request, stream and connection has a cancellation token held by the engine; quitting cancels and joins everything.
- A cancelled request reports `Event::Cancelled`, never a timeout or a silent drop.
- Timeouts are per request (connect, TLS, total) and come from settings, never hard-coded.
- Name every thread (`std::thread::Builder::name`); catch and log a panic per worker job.
- Tests use `#[tokio::test(start_paused = true)]` for timing.

| Excuse | Reality |
| --- | --- |
| "tokio is already a dependency" | Only the net crates may use it; `deny.toml` wrappers enforce that. |
| "the TUI can just await the response" | The loop must keep drawing and reading keys; responses arrive as messages. |
| "it finishes in the background after quit" | Then quit leaks a connection. Cancel and join. |
