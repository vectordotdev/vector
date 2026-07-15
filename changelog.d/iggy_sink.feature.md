Added a new `iggy` sink that publishes OTLP logs, metrics, and traces to
[Apache Iggy](https://iggy.apache.org/) topics.

The sink shards, encodes, and durably appends messages using a compact,
versioned wire format designed for a queue-only observability storage
ingest path, letting a single Vector agent replace a reference
OpenTelemetry Collector plus a bespoke OTLP-to-Iggy adapter. Pair it with
the `opentelemetry` source configured with `use_otlp_decoding: true` so
the OTLP structure is preserved end to end.

authors: alexpacio
