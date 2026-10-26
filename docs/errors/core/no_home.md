# `core::no_home`

pawsman cannot find a home directory for its settings and log.

## When it happens

`HOME` is unset or empty and no absolute `XDG_CONFIG_HOME` or `XDG_STATE_HOME` is set, as in some service managers and stripped `env -i` shells. pawsman still runs, without a log and with default settings.

## How to fix it

Set `HOME`, or set `XDG_CONFIG_HOME` and `XDG_STATE_HOME` to absolute paths.

## Related

- [`core::settings_invalid`](settings_invalid.md)
