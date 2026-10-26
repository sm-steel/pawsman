# `tui::no_tty`

pawsman found no terminal to draw on.

## When it happens

The process has no controlling terminal, so `/dev/tty` cannot be opened: it runs from cron or a service, under `ssh host pawsman` without `-t`, or with its terminal detached. Redirecting stdin or stdout alone does not cause it, since pawsman draws on `/dev/tty`.

## How to fix it

Run pawsman in a terminal emulator; over SSH use `ssh -t host pawsman`.

## Related

- [`tui::terminal_mode`](terminal_mode.md)
