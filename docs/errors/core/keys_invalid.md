# `core::keys_invalid`

`~/.config/pawsman/keys.toml` cannot be applied, so the default keys are used.

## When it happens

pawsman reads `keys.toml` at start (under `XDG_CONFIG_HOME` when set). The whole file is ignored when any line is wrong:

- an action name that does not exist (names are listed in the F1 overlay's order and in the README);
- a key that does not parse, such as `Hyperdrive-X`;
- an empty list for `quit` or `help`, which would leave no way out;
- one key on two actions that work in the same place, such as `trace = ["e"]` next to the default `e` for the environment picker;
- a table other than `[keys]`.

The message names the problem.

## How to fix it

Fix the file; until then the default keys apply. Keys are written as F1 shows them: `Ctrl-K`, `Alt-Enter`, `F5`, `Shift-Tab`, `PgDn`, `j`.

## Related

- [`core::settings_invalid`](settings_invalid.md)
