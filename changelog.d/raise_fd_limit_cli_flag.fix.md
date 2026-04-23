A new `--raise-fd-limit` CLI flag (or `VECTOR_RAISE_FD_LIMIT` environment variable)
raises the file descriptor soft limit (RLIMIT_NOFILE) to the hard limit at startup.
This prevents "Too many open files" errors when Vector monitors large numbers of log
files. On macOS, Vector falls back to the kernel-enforced `kern.maxfilesperproc` limit
if the hard limit is too high.

authors: vparfonov
