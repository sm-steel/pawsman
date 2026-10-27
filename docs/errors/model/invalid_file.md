# `model::invalid_file`

A collection or request file cannot be used.

## When it happens

The file is not valid TOML, has a key pawsman does not know (a typo such as `urll`), a value of the wrong type (a header that is not a string) or a `kind` other than `http`. It also happens when `pawsman.toml` has no integer `format`. A broken request file shows as an error entry in the sidebar; every other request still works.

## How to fix it

Correct what the message names, save, and open the collection again.

## Related

- [`model::not_a_collection`](not_a_collection.md)
- [`model::format_too_new`](format_too_new.md)
