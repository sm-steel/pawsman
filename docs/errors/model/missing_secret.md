# `model::missing_secret`

A request uses a secret that the active environment declares but nothing sets.

## When it happens

The environment file lists the name in `secrets = [...]`, a request uses `{{name}}`, and there is no value. pawsman looks in the process environment variable `PAWSMAN_SECRET_<NAME>` (the name uppercased, every other character turned into `_`), then in the collection's `.env`. The request is not sent: sending it with an empty or missing credential would only fail later, with a less clear error from the server.

## How to fix it

Set the variable before starting pawsman, for example `PAWSMAN_SECRET_TOKEN=… pawsman my-api`, or add `token=…` to the collection's `.env`, which pawsman keeps out of git.

## Related

- [`model::unknown_var`](unknown_var.md)
