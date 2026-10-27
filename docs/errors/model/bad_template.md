# `model::bad_template`

A `{{…}}` template in a request, folder, environment or variable value does not parse.

## When it happens

A `{{` has no closing `}}`, the braces are empty (`{{}}`), or a variable name has a character other than letters, digits, `_`, `.` and `-` (a `$` only as the first character, for built-ins such as `{{$uuid}}`). The message names the field and the byte offset where the template breaks. The request is not sent.

It also happens when a template keeps expanding: a field that would grow past 1 MiB, or variables nested more than 64 deep.

## How to fix it

Close the braces or fix the name. To send a literal `{{`, write `\{{`.

## Related

- [`model::unknown_var`](unknown_var.md)
- [`model::var_cycle`](var_cycle.md)
