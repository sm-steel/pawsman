# `tui::terminal_io`

Writing to or reading from the terminal failed while pawsman was running.

## When it happens

The terminal went away mid-session (a closed window, a dropped SSH connection that did not deliver `SIGHUP`), or the system ran out of file descriptors. pawsman restores the terminal modes it can and exits.

## How to fix it

Start pawsman again; if it repeats, check the log under `~/.local/state/pawsman/` for the underlying I/O error.

## Related

- [`tui::input_closed`](input_closed.md)
