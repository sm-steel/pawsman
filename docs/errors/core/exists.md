# `core::exists`

The file or folder pawsman would create already exists.

## When it happens

Renaming a request or folder moves its file or directory to a name made from the new name (`Create user` becomes `create-user.req.toml`). When that name is taken, pawsman stops instead of overwriting. `pawsman init` stops the same way when the directory already holds `pawsman.toml`.

## How to fix it

Pick another name, or move the other file away first.

## Related

- [`core::write`](write.md)
