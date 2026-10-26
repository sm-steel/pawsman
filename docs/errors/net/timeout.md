# `net::timeout`

The connection or the whole request took longer than allowed.

## When it happens

Connecting (DNS, TCP and TLS) took longer than `[http] connect_timeout_ms` (10 s by default), or the whole request, body included, took longer than `[http] timeout_ms` (30 s by default).

## How to fix it

Send again, or raise the limit in `~/.config/pawsman/settings.toml`.

## Related

- [`net::connect`](connect.md)
