The `file` source now recognizes a multi-byte `line_delimiter`, such as `\r\n`, when a read reaches the end of the file partway through the delimiter. Previously the line and the one after it were merged into one event, or both dropped if together they exceeded `max_line_bytes`. A line over `max_line_bytes` whose delimiter is split across two reads is also no longer emitted.

authors: vivek-raj1
