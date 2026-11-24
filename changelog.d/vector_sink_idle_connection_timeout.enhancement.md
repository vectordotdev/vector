The `vector` sink has a new `idle_connection_timeout_secs` option that sets how long a connection
is kept open for reuse after the last request started on it. It defaults to 90 seconds, as before.
Set it longer than the interval between requests to keep one connection, and avoid a new TLS
handshake, across batches that are spaced further apart than 90 seconds.

authors: hovinen
