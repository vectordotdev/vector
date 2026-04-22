Vector now automatically raises the file descriptor soft limit at startup and adds a
`max_open_files` option to the `file` source. When the limit is reached, the least recently
read file is closed with its checkpoint preserved, preventing "Too many open files" errors
in environments with many log files.

authors: vparfonov
