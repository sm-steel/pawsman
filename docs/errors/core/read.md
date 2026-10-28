# `core::read`

A collection file changed on disk but cannot be read.

## When it happens

The watcher saw a file change and pawsman could not read the new contents, usually because of its permissions. pawsman keeps the contents it read before, so the tree does not change.

## How to fix it

Check the file's permissions, then save it again; the watcher picks it up.

## Related

- [`core::watch`](watch.md)
