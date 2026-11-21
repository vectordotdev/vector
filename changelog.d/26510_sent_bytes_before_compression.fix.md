Sinks that use the shared request builder with compression enabled, such as `loki`, now report
`component_sent_bytes_total` as the payload size before compression, as the component specification
requires. Previously they reported the compressed size, so the metric was not comparable with sinks
such as `datadog_logs` that already reported the uncompressed size.

authors: ashokram-si
