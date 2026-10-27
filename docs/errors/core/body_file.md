# `core::body_file`

A file the request sends as its body cannot be read.

## When it happens

The request's `[body]` has `file = "…"`, or a multipart part has `file = "…"`, and pawsman cannot read that file when sending: it does not exist, is a directory, or is not readable. A relative path is taken from the folder of the request file, not from where pawsman was started. A file that resolves outside the collection, through a symlink for example, is refused, so a shared collection cannot upload your other files; absolute paths and `..` are already rejected when the request file is read. Nothing is sent.

## How to fix it

Correct the path, or put the file where the request expects it.

## Related

- [`model::missing_sidecar`](../model/missing_sidecar.md)
