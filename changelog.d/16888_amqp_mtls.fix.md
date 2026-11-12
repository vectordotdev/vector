Fixed mutual TLS (mTLS) authentication by properly passing client certificates in the `amqp` source and sink.

If a configuration specifies both `tls.crt_file` and a PKCS#12 archive in `tls.key_file`, it will now fail,
as this setup contradicts the documented TLS configuration.
Please see the [`amqp` source docs](https://vector.dev/docs/reference/configuration/sources/amqp/#tls) and [`amqp` sink docs](https://vector.dev/docs/reference/configuration/sinks/amqp/#tls).

authors: Captain1653
