# `net::tls`

The TLS handshake with the server failed.

## When it happens

The URL says `https` but the server speaks plain HTTP, the certificate is not trusted by the system store, has expired, or names another host.

## How to fix it

Check the URL scheme and the server's certificate; switching certificate checks off per collection comes with M3.

## Related

- [`net::connect`](connect.md)
