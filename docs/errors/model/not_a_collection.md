# `model::not_a_collection`

The directory pawsman was asked to open has no `pawsman.toml`.

## When it happens

`pawsman DIR` points at a directory that does not exist, or at one without a `pawsman.toml` at its top level, for example a subfolder of a collection.

## How to fix it

Open the collection's top directory, or create `pawsman.toml` there with `format = 1` and a `name`.

## Related

- [`model::invalid_file`](invalid_file.md)
