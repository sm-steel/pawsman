# `tui::terminal_mode`

The terminal refused a mode pawsman needs: raw input or the alternate screen.

## When it happens

`/dev/tty` is not a real terminal device (a pipe or a serial line without termios), or writing the mode's escape sequence failed. Optional modes (kitty keyboard flags, bracketed paste, focus events, mouse) never cause this error; a failure there is logged and the mode is skipped.

## How to fix it

Run pawsman in a full terminal emulator such as kitty, or inside tmux.

## Related

- [`tui::no_tty`](no_tty.md)
- [`tui::terminal_io`](terminal_io.md)
