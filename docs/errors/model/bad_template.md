# `model::bad_template`

A `{{…}}` template in a request, folder, environment or variable value does not parse.

## When it happens

A `{{` has no closing `}}`, the braces are empty (`{{}}`), or a variable name has a character other than letters, digits, `_`, `.` and `-` (a `$` only as the first character, for built-ins such as `{{$uuid}}`). The message names the field and the byte offset where the template breaks. The request is not sent.

It also happens when variables keep expanding: they would add more than 1 MiB to one field or 16 MiB to the whole request, or they nest more than 64 deep. Literal text does not count, so a large body without variables is fine.

## How to fix it

Close the braces or fix the name. To send a literal `{{`, write `\{{`.

## Related

- [`model::unknown_var`](unknown_var.md)
- [`model::var_cycle`](var_cycle.md)
