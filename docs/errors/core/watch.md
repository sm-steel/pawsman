# `core::watch`

pawsman cannot watch the collection for edits made outside it.

## When it happens

At start, pawsman asks the operating system to report changes in each folder of the collection (inotify on Linux, FSEvents on macOS). This fails when the system is out of watches (`fs.inotify.max_user_watches` on Linux) or the file system does not support them, as some network mounts do. The collection still opens, and pawsman's own changes still show; only edits made in another program do not appear by themselves.

## How to fix it

Press Ctrl-R to reload after editing files elsewhere. On Linux, raise `fs.inotify.max_user_watches` (for example with `sysctl`) and start pawsman again.

## Related

- [`core::read`](read.md)
