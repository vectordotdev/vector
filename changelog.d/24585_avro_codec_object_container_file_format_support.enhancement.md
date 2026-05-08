The Avro codec now supports reading and writing Avro Object Container File (OCF) encoded objects.

For **encoding**, OCF is available via the batch serializer interface (`BatchSerializerConfig` with
`codec: avro_ocf`), the same interface used by Arrow and Parquet. Each batch of events is written as
a single self-contained OCF file with an embedded schema and a randomly-generated sync marker, making
the output directly consumable by Spark, Flink, avro-tools, and other standard Avro tooling.
Compression support (`deflate`, `snappy`) is not yet implemented and will be added in a follow-up.

For **decoding**, OCF is supported via the `encoding: object_container_file` option on the Avro
deserializer. Note that OCF decoding requires a framer that delivers complete OCF payloads (e.g.
`bytes` or `length_delimited`); `newline_delimited` framing is not compatible. The
`strip_schema_id_prefix` option is incompatible with OCF and will now produce an error if combined.

The existing `datum` encoding mode for both encoding and decoding is unchanged.

authors: jlambatl
