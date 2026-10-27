# `model::format_too_new`

The collection was written in a format newer than this pawsman reads.

## When it happens

`pawsman.toml` has `format` greater than the newest version this build knows (currently `1`). A newer pawsman wrote the collection, and an older one cannot tell what the new keys mean, so it refuses the collection rather than guess.

## How to fix it

Update pawsman. Do not lower the number by hand: the files may use keys the older format does not have.

## Related

- [`model::invalid_file`](invalid_file.md)
