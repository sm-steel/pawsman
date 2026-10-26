# `tui::input_closed`

The thread that reads keys from the terminal stopped.

## When it happens

Reading `/dev/tty` hit end of file or an error, for example when the terminal was closed, or the input thread panicked. pawsman restores the terminal and exits rather than running without input.

## How to fix it

Start pawsman again; a panic is in the log under `~/.local/state/pawsman/`.

## Related

- [`tui::terminal_io`](terminal_io.md)
