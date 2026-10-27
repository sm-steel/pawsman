# `model::missing_sidecar`

A request keeps its body in a sidecar file that is not there.

## When it happens

The request's `[body]` has `json = { sidecar = "name.body.json" }` (or `text`), and no file of that name sits next to the request file. pawsman writes long or binary bodies to such a file to keep diffs readable; deleting or renaming it alone leaves the request without its body. The request shows as an error entry and is not sent.

## How to fix it

Restore the file from version control, rename it back, or replace the reference with the body inline.

## Related

- [`model::invalid_file`](invalid_file.md)
