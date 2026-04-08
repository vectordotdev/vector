The `datadog_traces` sink now includes `span_kind`, `is_trace_root`, `peer_tags`, and other missing dimensions in APM stats aggregation, matching the Datadog Agent's behavior. Previously, these fields were omitted, causing `span.kind` to appear as `internal` in Datadog APM dashboards.

authors: []
