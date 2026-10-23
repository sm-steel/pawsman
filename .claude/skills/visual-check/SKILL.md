---
name: visual-check
description: Use after any change to the pawsman TUI (layout, panes, tabs, response viewer, images, splash, colours, wide characters) and before claiming it works — renders the real views to PNG files to read
---

# Visual check for pawsman

The TUI cannot be verified by reading code: layout bugs (clipped panes, misaligned borders, wide-character splits, wrong colours) only show when rendered. Look at the pictures before claiming a UI change works.

## 1. Scenario PNG files (primary, deterministic)

```bash
cargo test -p pawsman-tui visual -- --ignored      # target/visual/tui-*.png
```

- Scenarios drive the real app on ratatui's `TestBackend` with key presses, assert behaviour, and render the buffer cell by cell to PNG.
- Cover at least 80×24 and 160×48, a light theme, 256 colours, and wide characters.
- **Kitty images are not real images in the PNG files**: each is a flat tint of its id with the id in its top-left cell. Check size, place and clipping there; check the look in a live run. Half blocks are pixel-exact.
- **Read every `tui-*.png`** with the Read tool. Box drawing and block elements are exact; colour emoji show as gaps.
- Splash frames are rendered at fixed timestamps with a fixed seed.

## 2. Live run

- Run the real binary in a kitty window and capture it, then again over `ssh -t localhost` (only when `ssh -o BatchMode=yes localhost true` works; never set up keys for it) and inside tmux.
- In a non-kitty terminal (foot or alacritty, if installed) `--images auto` must log the fallback mode.
- A locked screen makes captures inconclusive: wait and run again, never skip.

## 3. Crate boundaries

```bash
tools/check-deps.sh
```

Run after any dependency or module move between crates.
