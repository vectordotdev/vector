Added `tls.min_version` and `tls.max_version` to let supported sources and sinks require TLS 1.2+,
require TLS 1.3, or cap the version for compatibility. Configuring a range that includes TLS 1.3
also enables it on TLS listeners, subject to host policy. Both options are opt-in.

The `kafka`, `nats`, `amqp`, `mqtt`, `databend`, and `greptimedb_metrics` components cannot
restrict the TLS version themselves. They fail validation when a configured bound is narrower than
what their TLS implementation, or for `kafka` the host's OpenSSL policy, already guarantees.

A configured bound only narrows one already set by the host's OpenSSL configuration
(`MinProtocol`/`MaxProtocol` in `openssl.cnf`) and it never re-enables a version the host disables.
Vector refuses to start if no protocol version is left usable.

authors: sainad2222
