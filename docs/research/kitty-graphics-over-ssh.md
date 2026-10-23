# Kitty graphics over SSH

What the M0 probe PoC (`pawsman-poc/probe`) showed about the capability probe and kitty images. Rules that depend on it: [rules/tui.md §9](../../rules/tui.md).

## Probe

- Queries, DA1 last as terminator: kitty graphics `ESC _G i=31,s=1,v=1,a=q,t=d,f=24;AAAA ESC \`, keyboard `CSI ? u`, cell size `CSI 16 t`, DA1 `CSI c`.
- Replies are read from `/dev/tty` with `poll` until the DA1 reply arrives; on a timeout the reader keeps reading for a grace period so a late reply never reaches the key handler.
- The reply parser is a pure function over bytes; reply order does not matter.

| Case | Result |
| --- | --- |
| kitty 0.49.2, local | graphics, keyboard protocol and a 10×20 px cell reported; no sixel; 18 ms |
| PTY with nothing answering (`script`, stdin `/dev/null`) | default caps after 1.15 s (1 s timeout + 150 ms grace); no reply bytes leaked |

Probe timeout for M1: 1 s plus 150 ms grace. Over SSH the round trip adds the link's RTT, which stays far below 1 s on usable links; a slower link degrades to half blocks instead of hanging, **but a reply arriving after the grace period reaches the input**.

Open for M1 (issue #42); the tmux behaviour is measured in [Matrix](#matrix):

- **Inside tmux, DA1 is answered by tmux itself, at once**, while the passthrough-wrapped graphics query is answered later by the outer terminal, so the probe stops before that reply arrives. Inside tmux the probe must end on a second DA1 sent through passthrough, answered by the outer terminal after the graphics reply.
- The input parser discards probe-shaped replies (`APC G…`, `CSI ?…u`, `CSI 6;…t`, `CSI ?…c`) that arrive late, so a slow link never types them.

## ratatui-image 11.1.0

- Draws kitty images through Unicode placeholders (`U=1`, U+10EEEE cells) with direct transmission (`t=d`), so the image moves and clips as ordinary cells.
- **Sends raw RGBA (`f=32`)**: a 260×260 image is 270 KB of pixels and 360 KB of base64 on the wire with its defaults. It has opt-in zlib (`o=z`, `QueryStdioOptions::kitty_compression`), enabled only through its own probe; it has no PNG (`f=100`) path. The PNG of the same picture is 143 KB here (a noisy gradient, the worst case for PNG); flat UI art and screenshots compress far better.
- `Picker::from_query_stdio()` sends its own graphics query and DA1, writing to the terminal outside the main loop. It can be skipped (`Picker::from_fontsize` plus `set_protocol_type`), but then its compression is unavailable.

## Decision for M1

pawsman implements its own kitty encoder in `pawsman-tui` (`term::graphics`) instead of using ratatui-image for kitty:

- One probe, pawsman's own; no second query, and compression without depending on ratatui-image's probe.
- Images are scaled to the cell box first, then sent as PNG (`f=100`) or as zlib-compressed RGBA (`o=z`), whichever is smaller, in 4096-byte base64 chunks with `q=2`.
- Placeholders are drawn as ordinary cells, as ratatui-image does.
- ratatui-image remains an option for the sixel and half-block fallbacks, behind the same `Caps`.

## Matrix

Run from kitty 0.49.2 on the desktop into a throw-away Docker box (`pawsman-poc/matrix`: Debian trixie, OpenSSH, tmux 3.5a, xterm on Xvfb), over a real SSH connection.

| Row | Probe result | Time | Image |
| --- | --- | --- | --- |
| kitty, local | graphics, keyboard, 10×20 px cell | 18 ms | kitty, correct |
| kitty over SSH | graphics, keyboard, 10×20 px cell | 7–21 ms | kitty, correct |
| SSH → tmux, passthrough on, UTF-8 | graphics (through passthrough), no keyboard protocol | 19–21 ms | kitty, correct |
| SSH → tmux, passthrough on, **no UTF-8 locale** | graphics | 20 ms | **broken**: tmux replaces every `U+10EEEE` placeholder with `_` |
| SSH → tmux, passthrough off | no graphics; tmux's own DA1 claims sixel | 150 ms | ratatui-image still picked kitty and drew `_` rows |
| xterm (`-ti vt340`) on Xvfb | sixel, no cell size (`CSI 16 t` unanswered) | 0.2 ms | sixel, correct |

Findings:

- **tmux needs UTF-8.** Over SSH the session usually has no `LANG`; tmux then rewrites non-ASCII cells (placeholders, and also half blocks and box drawing). `tmux -u` or a UTF-8 `LANG` fixes it; tmux's `RGB` terminal feature does not matter.
- **Inside tmux only the outer terminal's DA1 counts.** tmux answers the first DA1 itself and claims sixel even when the outer terminal (kitty) cannot show sixel.
- **The two-DA1 probe works**: inside tmux the passthrough-wrapped graphics query is answered, then the outer DA1, in about 20 ms. Without passthrough the second DA1 never comes; waiting only a grace period after tmux's own DA1 ends the probe in 150 ms instead of 1.15 s. The grace must cover the SSH round trip, so M1 uses 500 ms inside tmux.
- **ratatui-image's own detection is wrong under tmux without passthrough**: it chose kitty and drew garbage, which pawsman's probe-driven choice avoids.
- xterm answers DA1 with sixel (attribute 4) but not `CSI 16 t`; the cell size then comes from `TIOCGWINSZ` pixel fields or a default.
