# `net::spill`

A response body larger than the in-memory cap could not be written to its temp file.

## When it happens

The temp directory is full or not writable. Bodies above `[http] body_cap_bytes` (16 MiB by default) go to a temp file.

## How to fix it

Free some disk space in the temp directory, or raise the cap.

## Related

- [`net::protocol`](protocol.md)
