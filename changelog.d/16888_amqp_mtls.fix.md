Fixed mutual TLS (mTLS) authentication by properly passing client certificates in the `amqp` source and sink.

A configuration that specifies both `tls.crt_file` and a PKCS#12 archive in `tls.key_file` now fails,
because that setup contradicts the documented TLS configuration.
For more information, see the [`amqp` source docs](https://vector.dev/docs/reference/configuration/sources/amqp/#tls) and [`amqp` sink docs](https://vector.dev/docs/reference/configuration/sinks/amqp/#tls).
authors: Captain1653
