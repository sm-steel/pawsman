# `core::write`

A collection file cannot be written.

## When it happens

pawsman writes a file when you create, rename, delete or reorder something in the tree, when it remembers the active environment in `.pawsman/state.toml`, and when `pawsman init` creates a collection. Each write goes to a temporary file in the same folder, which then replaces the target, so a failed write never leaves a half-written file. It fails when the folder is not writable, the disk is full, or the path would leave the collection. The file on disk is unchanged.

## How to fix it

Check the folder's permissions and free space, then try again.

## Related

- [`core::exists`](exists.md)
- [`core::read`](read.md)
