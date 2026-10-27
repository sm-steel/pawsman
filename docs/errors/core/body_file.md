# `core::body_file`

A file the request sends as its body cannot be read.

## When it happens

The request's `[body]` has `file = "…"`, or a multipart part has `file = "…"`, and pawsman cannot read that file when sending: it does not exist, is a directory, or is not readable. A relative path is taken from the folder of the request file, not from where pawsman was started. Nothing is sent.

## How to fix it

Correct the path, or put the file where the request expects it.

## Related

- [`model::missing_sidecar`](../model/missing_sidecar.md)
