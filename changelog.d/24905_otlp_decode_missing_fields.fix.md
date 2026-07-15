When using Vector's native event format (i.e. `use_otlp_decoding` is
`false`, the default), the OpenTelemetry source now decodes previously
dropped OTLP protobuf fields across all signal types, fixing round-trip
data loss when events pass through Vector between OTLP endpoints.

- **Logs:** `ScopeLogs.schema_url`, `ResourceLogs.schema_url`, and
  `Resource.dropped_attributes_count` are now preserved.
- **Traces:** the full `InstrumentationScope` (name, version,
  attributes, `dropped_attributes_count`), both `schema_url` fields,
  and `Resource.dropped_attributes_count` are now decoded.
- **Metrics:** `scope.dropped_attributes_count`, both `schema_url`
  fields, and `resource.dropped_attributes_count` are now included as
  metric tags.

authors: szibis
