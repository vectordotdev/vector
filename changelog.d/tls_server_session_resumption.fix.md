TLS-enabled sources with `tls.verify_certificate: true` no longer fail the handshake of clients
that resume a previous TLS session. Such clients previously had their connection rejected with a
`session id context uninitialized` error.

authors: guilload
