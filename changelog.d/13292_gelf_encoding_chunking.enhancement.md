The `gelf` encoding format now supports [chunking](https://go2docs.graylog.org/current/getting_in_log_data/gelf.html#chunking) when used with the `socket` sink in `udp` mode. The maximum chunk size can be configured using `encoding.gelf.max_chunk_size`.

authors: aramperes
