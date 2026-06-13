The `file` source now supports a `max_open_files` option that limits the number
of simultaneously open file handles. When the limit is reached, the least recently
read file is closed with its checkpoint preserved, preventing "Too many open files"
errors in environments with many log files. If not explicitly configured, Vector
auto-derives a default from the OS file descriptor limit (80% of RLIMIT_NOFILE on Unix).

authors: vparfonov
