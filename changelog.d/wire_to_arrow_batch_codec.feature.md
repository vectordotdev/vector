Added a `wire_to_arrow` batch encoding codec that decodes protobuf wire bytes from each
event's `message` field directly into an Apache Arrow `RecordBatch`, pairing proto fields to
Arrow columns by name. This bypasses the generic `ProtobufDeserializer -> Event ->
ArrowStreamSerializer` chain for sinks that already carry raw proto bytes, avoiding the
intermediate `DynamicMessage` / `LogEvent` representations. The codec is configured with a
proto descriptor (`desc_file` + `message_type`) for the incoming bytes; the sink injects the
output Arrow schema. Malformed rows are isolated (dropped and counted via the
`wire_to_arrow_rows_dropped` metric) rather than failing the whole batch.

authors: amandaLi7
