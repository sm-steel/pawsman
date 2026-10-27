# `core::globals_invalid`

`~/.config/pawsman/globals.toml` cannot be used.

## When it happens

The file is not valid TOML, has a table other than `[vars]`, or a variable whose value is not a string. pawsman shows a notice and opens the collection without global variables; a request that needs one then fails with [`model::unknown_var`](../model/unknown_var.md).

## How to fix it

Correct what the message names. The file holds one table:

```toml
[vars]
user_agent = "pawsman"
```

## Related

- [`core::settings_invalid`](settings_invalid.md)
