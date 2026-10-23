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

Open for M1 (issue #42):

- **Inside tmux, DA1 is answered by tmux itself, at once**, while the passthrough-wrapped graphics query is answered later by the outer terminal, so the probe stops before that reply arrives. Inside tmux the probe must end on a second DA1 sent through passthrough, answered by the outer terminal after the graphics reply.
- The input parser discards probe-shaped replies (`APC G…`, `CSI ?…u`, `CSI 6;…t`, `CSI ?…c`) that arrive late, so a slow link never types them.
- The PoC never wrapped the query in passthrough; the tmux expectation below is untested.

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

## Not yet run

These rows need setup that this machine lacks; the plan's matrix stays open for them (issue #5):

| Row | Needs |
| --- | --- |
| kitty over SSH | key-based `ssh localhost` (not set up here; never set up automatically) |
| tmux with and without `allow-passthrough`, SSH then tmux | tmux (not installed) |
| a non-kitty terminal | foot, alacritty or another terminal (none installed) |

Expected from the protocol and the code: over SSH nothing changes except latency (`t=d` needs no shared files); inside tmux without passthrough the graphics query gets no reply, so the probe falls back to half blocks.
