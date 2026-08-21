Added an optional `tls_handshake_timeout_secs` setting to the `socket` (TCP mode), `syslog` (TCP mode), `logstash`, `fluent`, and `statsd` (TCP mode) sources. When set, a TLS-enabled connection that does not complete its TLS handshake within the configured number of seconds is closed.

Previously, TLS handshakes on these sources had no timeout: a client that opened a TCP connection and never completed (or never started) the TLS handshake would hold its slot against `connection_limit` indefinitely, since neither TCP keepalive nor `max_connection_duration_secs` are evaluated until after the handshake succeeds. This could let misbehaving or unresponsive clients gradually exhaust the connection limit and block legitimate traffic. The new setting is unset by default, preserving prior behavior.

authors: vladimir-dd
