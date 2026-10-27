# `net::tls`

The TLS handshake with the server failed.

## When it happens

The URL says `https` but the server speaks plain HTTP, the certificate is not trusted by the system store, has expired, or names another host. It also happens for every `https` URL on a machine with no system CA certificates (the message then says `no trusted certificates`); plain `http` still works there.

## How to fix it

Check the URL scheme and the server's certificate. Without system certificates, install the CA bundle (`ca-certificates` on Debian and Ubuntu); switching certificate checks off per collection comes with M3.

## Related

- [`net::connect`](connect.md)
