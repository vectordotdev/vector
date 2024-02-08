When terminating idle HTTP connections using the configured `max_connection_age`, only send
`Connection: Close` for HTTP/0.9, HTTP/1.0, and HTTP/1.1 requests. This header is not supported on
HTTP/2 and HTTP/3 requests. This may be supported on these HTTP versions in the future.
