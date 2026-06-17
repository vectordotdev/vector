Fixed a `disk` buffer (v2) crash loop where, after a crash or forced restart, Vector
could fail to reopen the buffer with `failed to seek to position where reader left off:
No such file or directory` and exit with a configuration error on every restart. The
reader now advances past a fully acknowledged data file that was already deleted instead
of failing the buffer build, so the buffer always reopens and continues delivering.

Disk buffer (v2) durability was also hardened: the directory holding the buffer is now
`fsync`ed after a data file is created. Previously only file contents were synced, so a
crash could lose a freshly created data file's directory entry and drop data that had
been reported as synced to disk.
