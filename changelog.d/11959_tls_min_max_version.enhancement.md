Components that use Vector's `tls` configuration block now support `min_tls_version` and
`max_tls_version`, which constrain the TLS protocol versions Vector will negotiate. Accepted
values are `TLSv1`, `TLSv1.1`, `TLSv1.2`, and `TLSv1.3`.

Previously the negotiated version was fixed by the underlying library defaults, which still permit
the deprecated TLS v1.0 and v1.1. Setting `min_tls_version: TLSv1.2` refuses them, which resolves
the findings commonly raised by TLS compliance scanners against Vector's listening sources.

Components that accept connections also did not offer TLS v1.3, because the acceptor is built from
Mozilla's v4 intermediate profile which disables it. Setting either option enables every version
inside the resulting window, so `min_tls_version: TLSv1.2` makes TLS v1.3 available as well.

Both options are optional and unset by default, so existing configurations negotiate exactly the
same versions as before.

These options take effect for every component whose `tls` settings are applied to an OpenSSL
context, which includes the HTTP-based sources and sinks and the AWS SDK-based sinks.

Some components pass the configured certificates to a third-party TLS implementation instead, which
does not expose protocol version selection. Those components cannot enforce the bounds, so they now
log a warning when either option is set rather than ignoring it silently: the `kafka` source and
sink (librdkafka), the `nats` source and sink (async-nats), the `amqp` source and sink (lapin), the
`mqtt` source and sink (rumqttc), and the `gcp_pubsub` source (tonic). The `greptimedb_metrics` sink
lists them in its existing unsupported-options warning.

authors: sainad2222
