The OpenTelemetry source now decodes previously dropped OTLP protobuf
fields across all signal types. For logs: `ScopeLogs.schema_url`,
`ResourceLogs.schema_url`, and `Resource.dropped_attributes_count` are
now preserved. For traces: the entire `InstrumentationScope` (name,
version, attributes, dropped_attributes_count), both `schema_url`
fields, and `Resource.dropped_attributes_count` are now decoded. For
metrics: `scope.dropped_attributes_count`, both `schema_url` fields,
and `resource.dropped_attributes_count` are now included as metric
tags. This fixes round-trip data loss when events pass through Vector
between OTLP endpoints.

authors: szibis
