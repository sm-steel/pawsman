# `net::start`

Networking could not start.

## When it happens

The tokio runtime thread could not be created (the process is out of threads or memory), or the TLS setup could not load the system's certificate store.

## How to fix it

Check the log under `~/.local/state/pawsman/` for the reason; on Linux make sure a CA bundle is installed (`ca-certificates`).

## Related

- [`net::tls`](tls.md)
