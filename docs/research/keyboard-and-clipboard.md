# Keyboard and clipboard

What the M0 keys PoC (`pawsman-poc/keys`, crossterm 0.29.0) showed in kitty 0.49.2. Rules that depend on it: [rules/tui.md §2, §5, §8](../../rules/tui.md).

## Keys

| Key | Kitty keyboard protocol on | Legacy (protocol not pushed) |
| --- | --- | --- |
| Ctrl-Enter | `Enter` + CONTROL | plain `Enter`: indistinguishable |
| Alt-Enter | `Enter` + ALT | `Enter` + ALT |
| Shift-Enter | `Enter` + SHIFT | not distinguishable |
| Ctrl-I / Tab | `Char('i')` + CONTROL vs `Tab` | both `Tab` |
| Esc | `Esc`, immediately | `Esc` |
| Ctrl-C | `Char('c')` + CONTROL (raw mode) | same |
| Ctrl-Q | `Char('q')` + CONTROL | same, even with `stty ixon` set beforehand: raw mode clears IXON |
| Paste | one `Paste(text)` event (bracketed paste) | same |

- With `REPORT_EVENT_TYPES`, every key arrives twice (Press, Release); handlers act on Press and Repeat only.
- **Flags: `DISAMBIGUATE_ESCAPE_CODES` only.** With `REPORT_ALL_KEYS_AS_ESCAPE_CODES`, crossterm 0.29 reports Shift+1 as `Char('1')` + SHIFT instead of `!` unless the alternate-key field is present, so text fields would type the wrong characters. Under `DISAMBIGUATE` alone, kitty sends text-producing keys (shifted included) as text, and Ctrl/Alt chords stay distinguishable. The PoC's synthetic keys (`kitty @ send-key shift+a`) still arrive as `Char('a')` + SHIFT under either flag set; real typing is checked in #42.
- Layout folding (a Cyrillic key acting as the US key in the same place) cannot use kitty's base-layout field, which crossterm drops; pawsman keeps its own folding table. `ж` arrives as `Char('ж')`.
- crossterm's `supports_keyboard_enhancement()` writes its own query; M1 takes the keyboard capability from pawsman's probe instead.
- Bindings that need the protocol get legacy alternatives: Alt-Enter for Ctrl-Enter (send), never Ctrl-I.

## Mode stack

`raw → alternate screen → kitty keyboard flags → bracketed paste → focus events → mouse`, undone newest first (mouse, focus, paste, keyboard, alternate screen, raw) from `restore()`, `Drop` and the panic hook.

For M1: the panic hook must not block on the mode lock (`try_lock`), and a panic on a worker thread must not restore the terminal under a running main loop; it ends the app instead.

| Exit | Shell afterwards |
| --- | --- |
| Ctrl-Q | cooked mode, echo on, `↑` arrives as legacy `^[[A`, `ixon` as before |
| `panic!` (F12) | the same: panic message printed on the main screen, keys legacy, echo on |

## Mouse

crossterm's `EnableMouseCapture` turns on any-motion tracking (mode 1003): every mouse move over the window is an event. pawsman enables button-event tracking only (`CSI ? 1000 h`, `CSI ? 1002 h`, `CSI ? 1006 h`: clicks, wheel, drag) and undoes exactly those, so a moving mouse never wakes the loop.

## Clipboard

- OSC 52 copy (`crossterm::clipboard::CopyToClipboard`) reached the Wayland clipboard in kitty with its default `clipboard_control` (writes allowed).
- Inside tmux the sequence is wrapped in DCS passthrough (`ESC P tmux; ESC <osc> ESC \`); not run here (no tmux on the test machine).
- Over SSH the sequence is identical; the terminal, not the remote host, owns the clipboard.

## Not yet run

tmux, SSH and a non-kitty terminal: same gaps as [kitty graphics over SSH](kitty-graphics-over-ssh.md#not-yet-run).
