# Terminal client rules

Rules for `crates/pawsman-tui`. [rust-project.md](rust-project.md) wins every conflict. Anything that is not drawing or input lives in `pawsman-core`.

1. **The terminal belongs to the app.**
   - Never write to stdout or stderr directly; `std::io::stdout`/`stderr` are banned by clippy.
   - Output goes through `term::Term`: frames, graphics, OSC 52, the title, the bell.
   - The single exception is the one exit line printed in `main` after the terminal has been restored. `pawsman run` never starts the TUI; its report writer owns stdout and stderr.
   - Logs go to the log file only.
2. **Every mode is undone.**
   - Each mode the app turns on is listed in `term::MODES` and undone in `Term::restore`, newest first: mouse tracking (button events only: `CSI ? 1000 h`, `1002`, `1006`; never any-motion `1003`), focus events, bracketed paste, kitty keyboard flags, alternate screen, raw mode.
   - `restore` runs on a normal exit, on an error, from `Drop`, and from the panic hook.
   - If a setup step fails, the steps already done are undone, newest first, before the error returns. A failing optional mode is logged and skipped.
   - `$EDITOR` (Ctrl-E) suspends: every mode is undone before the editor starts and set up again after it exits.
   - Never add a mode without adding its undo.
3. **Only the main loop touches the terminal.** The engine, input, image and clipboard threads send `Msg`s to the loop.
4. **No idle redraws.**
   - Draw after an input, an engine event, a resize or a notice. Every queued message is handled before the one frame, so a burst draws once.
   - A timer redraws only while something time-based is on screen: the splash, a running request's elapsed time, a live stream, a notice that expires.
5. **One key table.**
   - The table lives in `pawsman_core::keys`; the hint bar and the F1 help overlay are generated from the same rows; user overrides come from `~/.config/pawsman/keys.toml`.
   - F1 is the help key everywhere; Esc or F1 closes it.
   - Typed characters are never commands while a text field has focus.
   - A Ctrl, Alt, Super, Hyper or Meta chord the table does not bind types nothing; Ctrl+Alt types (AltGr).
   - **Ctrl-Q quits.** Ctrl-C copies the selection and, with nothing selected, does nothing but show a hint naming Ctrl-Q. Ctrl-V pastes through bracketed paste. Raw mode clears IXON, so Ctrl-Q arrives as a key in legacy terminals.
   - Ctrl-Enter sends; Alt-Enter and F5 are the fallbacks where Ctrl-Enter is indistinguishable from Enter (tmux, legacy terminals); F5 works everywhere, including xterm, which drops Alt-Enter.
   - Esc steps back one level: an overlay, then a field edit, then focus to the parent pane.
   - Letters match after layout folding (a Cyrillic key acts as the US key in the same place), only where a command is allowed, never in text.
6. **Measure in cells, by grapheme.** Widths come from unicode-width per grapheme; never count `chars()` or bytes for layout. A wide character never splits, and truncation adds "…".
7. **Focus is visible.** The focused pane's border takes the accent; terminal focus events dim the UI when the terminal loses focus.
8. **Clipboard.** Copy goes out as OSC 52 through `Term::copy`, which works in kitty and over SSH, wrapped in DCS passthrough inside tmux. arboard runs on its own thread and only in local sessions; over SSH a read says the clipboard is not reachable and points to the terminal's own paste.
9. **Images.**
   - Modes: Kitty (graphics protocol through Unicode placeholders), Sixel, Blocks (half blocks), Off (a text placeholder). `--images auto|kitty|sixel|blocks|off`; `auto` uses the probe.
   - **The probe runs before input and before the alternate screen**: after raw mode and the panic hook, before the alternate-screen switch and the input thread. Inside tmux the passthrough replies do not arrive once the alternate screen is on. It writes the kitty graphics query, the keyboard query (`CSI ? u`), `CSI 16 t`, a DECRQSS SGR query for truecolor (with `COLORTERM` from the env snapshot) and DA1, reads the replies from `/dev/tty` with a timeout, then reads briefly more to swallow late replies. Inside tmux it ends on a second DA1 sent through passthrough, because tmux answers the first DA1 itself, and waits at most 500 ms for it after tmux's own; tmux's own DA1 never decides sixel. The input parser also drops probe-shaped replies that arrive later, so no reply byte reaches the key handler. The parser is a pure function with its own tests. Timeouts come from [docs/research/kitty-graphics-over-ssh.md](../docs/research/kitty-graphics-over-ssh.md).
   - Inside tmux, graphics are wrapped in DCS passthrough. tmux without a UTF-8 locale rewrites placeholders and half blocks, so then images are Off and the hint bar says to run tmux with `-u` or a UTF-8 `LANG`.
   - **Placeholders are ordinary cells**: `U+10EEEE` plus row and column diacritics, with the image id as the truecolour foreground. Scrolling, clipping and overlays need nothing else. A placeholder cell is never selectable or copyable.
   - **Images are sent before the frame that shows them**, inline (`t=d`, base64 chunks of 4096 bytes, `q=2`), so SSH works and nothing answers on the input.
   - **Every image is deleted**: `a=d,d=I` when it leaves the cache, `a=d,d=A` once in `Term::restore` and in the panic hook while the alternate screen is still shown.
   - **Decoding is off the loop**, on the image thread, within size limits; a panic is caught per job.
   - **Sizes never jump**: the box comes from the image's dimensions and the cell size before decoding finishes.
10. **Startup animation.**
    - A splash of about 1.2 s plays after the probe and before the main view; any key skips it to its final frame; Ctrl-Q still quits.
    - Cell-based tachyonfx effects only; a kitty logo, if shown, is static.
    - Frames are drawn only while the effect runs; then the loop blocks on input.
    - Without truecolor, RGB cells map to indexed colours; on 16 colours or `TERM=dumb` the final frame shows once; below the logo size there is no splash.
    - `--no-splash`, `ui.animations = false` and `PAWSMAN_NO_ANIMATION` turn it off. See [docs/research/startup-animation.md](../docs/research/startup-animation.md).
11. **Checks.**
    - `cargo test -p pawsman-tui`: views on `TestBackend`.
    - `cargo test -p pawsman-tui visual -- --ignored`: PNG files in `target/visual/tui-*.png`, read before claiming a UI change works (the visual-check skill).
    - PTY tests replay canned terminal replies for every probe and fallback path, including Ctrl-Q in a legacy terminal.
