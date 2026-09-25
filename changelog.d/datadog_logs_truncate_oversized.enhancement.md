Oversized events sent through the `datadog_logs` sink are now reduced and delivered instead of
being dropped. The sink preserves reserved Datadog fields, truncates the message, adds a
`truncated:true` tag, and reports truncations through the
`datadog_logs_events_truncated_total` internal metric.

authors: arun-pidugu_ddog
