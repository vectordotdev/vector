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

These options apply to components whose TLS settings are applied to an OpenSSL context. Components
that hand the configured certificates to a third-party TLS stack cannot honor them: the `mqtt`
source and sink, and the `gcp_pubsub` source, now log a warning when either option is set, and the
`greptimedb_metrics` sink includes them in its existing unsupported-options warning. Components that
do not read Vector's `tls` block at all, such as `kafka` (librdkafka) and the AWS SDK-based sinks,
are unaffected.

authors: sainad2222
