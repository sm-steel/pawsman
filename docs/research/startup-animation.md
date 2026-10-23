# Startup animation

How ratatui projects animate a splash, and what pawsman uses. Versions and licences checked on crates.io on 2026-10-23.

## Decision

Cell-based animation with [tachyonfx](https://github.com/ratatui/tachyonfx), about 1.2 s, any key skips, drawing stops when the effect is done. A kitty image logo, when used, is transmitted once and shown static; kitty's own frame animation is not used.

## Crates

| Crate | Version | Licence | Use |
| --- | --- | --- | --- |
| tachyonfx | 0.25.2 | MIT | Effects over a ratatui `Buffer`: `coalesce`, `fade_from_fg`, `hsl_shift_fg`, `sweep_in`, combinators, spatial patterns, easings. `default-features = false, features = ["std"]` drops the DSL parser |
| tui-big-text | 0.8.10 | MIT OR Apache-2.0 | Optional wordmark; a hand-made `const` logo needs no dependency |
| ratatui-image | 11.1.0 | MIT | Optional static kitty logo. Uses `t=d` with Unicode placeholders and tmux passthrough. `default-features = false, features = ["crossterm"]`: the default `chafa-dyn` links LGPL libchafa |

Rejected: cfonts (GPL-3.0), tui-rain (ratatui 0.29 only), ratatui-splash-screen (sleep-driven loop).

## Reference projects

| Project | Technique | Worth reusing |
| --- | --- | --- |
| [maki](https://github.com/tontinton/maki) `maki-ui/src/splash.rs` | Hand-rolled ease-out fade and sine-wave field | Render is a pure function of `start.elapsed()`; the widget reports a cadence (16 ms while animating, idle after), so the loop goes back to blocking reads |
| [glim](https://github.com/junkdog/glim) | tachyonfx `parallel`/`sequence` of fades, coalesce and sweeps scoped by `CellFilter` | Skip by processing effects with a one-hour delta: every effect jumps to its final frame |
| [splashboard](https://github.com/unhappychoice/splashboard) | tachyonfx 0.25, ratatui 0.30, ratatui-image without chafa | Frame as a function of the timestamp, tested through `TestBackend` |

## Kitty frame animation vs cells

- Kitty supports terminal-side animation (`a=f` frames, `a=a` playback), and its spec recommends that over client-driven redraws for SSH latency.
- Every frame must be uploaded before playback: estimated hundreds of KB to several MB for a short logo reveal, which can arrive after the user has pressed a key.
- Cell animation sends only changed cells (estimated under 8 KB per frame for a 40×8 logo) and behaves the same in kitty, sixel and half-block terminals, in tmux and in `TestBackend`.
- Not verified: `a=f`/`a=a` support in Ghostty, WezTerm and Konsole; animation of virtual placements (`U=1`); tmux relaying frame commands.

## Design

```mermaid
stateDiagram-v2
  [*] --> Probe
  Probe --> Splash: TTY, area fits logo, animations on
  Probe --> Main: otherwise
  Splash --> Main: effect done
  Splash --> Main: any key (effects jump to final frame)
  Splash --> [*]: Ctrl-Q
  Main --> [*]: Ctrl-Q
```

- The effect: the logo coalesces in, fades up from the background, then a hue sweep crosses it; the tagline sweeps in; a short fade hands off to the main view.
- While the effect runs the loop polls every 16 ms and draws; after it ends the loop blocks on input (no idle redraws).
- The splash never waits on startup work; collections load behind it.
- Off by `--no-splash`, `ui.animations = false` or `PAWSMAN_NO_ANIMATION`, read through the env snapshot.
- Without truecolor, RGB cells map to indexed colours after processing; on 16 colours or `TERM=dumb` the final frame shows once.
- Random effects take a seeded RNG (`SimpleRng::new(seed)`), so frames are deterministic.

## Testing

- insta snapshots of `TestBackend` buffers at 0, 300, 600, 900 and 1200 ms with a fixed seed.
- Skip test: after a key, the effect is done and the next frame equals the final snapshot.
- Cadence test: once done, the loop has no pending timer.
- 256-colour test: no `Color::Rgb` remains.
- Visual PNG files reviewed before merge.
