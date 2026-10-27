# `model::unknown_var`

A request uses a `{{name}}` that no scope defines.

## When it happens

pawsman looks a name up in order: runtime values, the request's `[vars]`, each folder's `_folder.toml` from the nearest up, the active environment, `pawsman.toml`, and `~/.config/pawsman/globals.toml`. `{{env.NAME}}` reads the process environment and nothing else, and `{{$…}}` names a built-in (`$uuid`, `$timestamp`, `$isoDate`, `$randomInt`). When none of these has the name, the request is not sent: an empty value would send a wrong request without saying so. The message names the variable and the field it is in.

The most common cause is having no environment picked, or the wrong one.

## How to fix it

Pick an environment with `e`, or define the variable in one of the scopes above. Check the spelling: names are case-sensitive.

## Related

- [`model::var_cycle`](var_cycle.md)
- [`model::missing_secret`](missing_secret.md)
- [`model::bad_template`](bad_template.md)
