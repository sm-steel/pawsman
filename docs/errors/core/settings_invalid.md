# `core::settings_invalid`

`settings.toml` cannot be read or is not valid TOML with known keys.

## When it happens

The file at `~/.config/pawsman/settings.toml` (or under `XDG_CONFIG_HOME`) has a syntax error, a misspelt or unknown key, a value of the wrong type, or cannot be read. pawsman shows the error as a notice and runs with the default settings.

## How to fix it

Correct the key or value the message names, or remove the file to use the defaults. Known keys: `[ui] animations` (true or false) and `[ui] images` (`auto`, `kitty`, `sixel`, `blocks` or `off`).

## Related

- [`core::no_home`](no_home.md)
