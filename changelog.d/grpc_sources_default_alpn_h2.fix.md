gRPC-based sources (e.g. `opentelemetry`) now default to advertising `h2` via ALPN when TLS is
enabled, unless `tls.alpn_protocols` is already set. Previously, gRPC clients that require the
server to confirm `h2` via ALPN (RFC 7540 Section 3.3) could fail the TLS handshake with
`Cannot check peer: missing selected ALPN property.`, since none of these sources expose an
`alpn_protocols` config option for users to set it themselves.

authors: vladimir-dd
