---
what: "Passing a directory to `tls.ca_file` for Kafka connections"
deprecated_since: "0.63.0"
---

Passing a directory path to `tls.ca_file` for a Kafka connection (`kafka` source and `kafka` sink) is deprecated.
Combine the CA certificates into a single PEM/DER file or inline value and pass it to `tls.ca_file` instead.
For more information, see the [`kafka` source docs](https://vector.dev/docs/reference/configuration/sources/kafka/#tls) and [`kafka` sink docs](https://vector.dev/docs/reference/configuration/sinks/kafka/#tls).
