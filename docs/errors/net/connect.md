# `net::connect`

The TCP connection to the server failed.

## When it happens

Nothing listens on that host and port (connection refused), the host is unreachable, or a firewall drops the connection.

## How to fix it

Check that the server is running and that the URL's port is right.

## Related

- [`net::dns`](dns.md)
- [`net::timeout`](timeout.md)
