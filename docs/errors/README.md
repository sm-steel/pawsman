# Error messages and pages

How pawsman errors are worded, and the template for each code's page. Codes and the registry: [CODE-ARCHITECTURE §4](../CODE-ARCHITECTURE.md#4-errors).

## Message (`Display`)

What went wrong, following the Rust API guidelines ([C-GOOD-ERR](https://rust-lang.github.io/api-guidelines/interoperability.html#error-types-are-meaningful-and-well-behaved-c-good-err)):

| Rule | Example |
| --- | --- |
| lowercase, no trailing punctuation, concise, one line | ``tls handshake with `api.example.com` timed out`` |
| names the bad input in backticks | ``unknown variable `token` in `users/get-user.req.toml` `` |
| says what is wrong, not how to fix it (that is the help line) | not ``unknown variable `token`, add it to `dev.env.toml` `` |
| no `error:` or `failed to` prefix: the renderer shows the code | ``request `Get user` cancelled`` |

## Help line (`help()`)

| Rule | Example |
| --- | --- |
| one sentence, imperative, starts with a verb, ends with a period | ``Define `token` in the active environment or in `.env`.`` |
| commands and values in backticks | |
| omitted when there is nothing actionable to say | |

## Page template

One page per code at `docs/errors/<namespace>/<slug>.md` (`ErrorCode::namespace` and `ErrorCode::slug`), e.g. `model/unknown_var.md` for `model::unknown_var`:

```markdown
# `model::unknown_var`

A request uses a variable that no scope defines.

## When it happens

## How to fix it

## Related
```

The summary line restates the message as a sentence.
