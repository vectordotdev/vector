Fixed TLS session resumption for sources that verify client certificates (`tls.verify_certificate: true`).
Previously, the source aborted every resumption attempt with a fatal `internal_error` alert, so clients
which resume sessions, including Vector's own sinks, logged `SSL alert number 80` and had to retry with
a full handshake each time they reconnected.

authors: hovinen
