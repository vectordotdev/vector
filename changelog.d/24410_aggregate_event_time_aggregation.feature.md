The `aggregate` transform now supports **event-time aggregation** via the new `time_source` configuration option. When set to `EventTime`, metrics are bucketed based on their embedded timestamps rather than processing time.

Additional configuration options include:

- `allowed_lateness_ms`: Grace period for accepting late-arriving events within event-time windows
- `use_system_time_for_missing_timestamps`: Option to fallback to system time or drop events without timestamps
- `max_future_ms`: Maximum allowed future timestamp offset to reject events with clock skew

This addresses issues where events with different source timestamps but same processing time would be incorrectly split across aggregation windows.

authors: kaarolch
