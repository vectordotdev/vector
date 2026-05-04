The `aggregate` transform now supports **event-time aggregation** via the new `time_source` configuration option. When set to `EventTime`, metrics are bucketed by their embedded timestamps rather than by processing time. This addresses cases where events with different source timestamps but the same processing time were incorrectly collapsed together (e.g. losing samples in sinks like Datadog that overwrite earlier values for an identical timestamp).

Additional configuration options for event-time mode:

- `allowed_lateness_ms`: Grace period that delays closing of an event-time window so late-arriving events still land in the correct bucket. Once a window is emitted, it is closed and cannot be reopened by later events.
- `use_system_time_for_missing_timestamps`: Fall back to system time for events that do not carry a timestamp; otherwise such events are dropped.
- `max_future_ms`: Maximum allowed future-timestamp offset before an event is dropped (clock-skew guard).

Behaviour:

- Late events for a window that has already been emitted are dropped (and counted via `component_discarded_events_total`) rather than re-opening the window. The watermark advances to the exclusive end of the latest emitted window.
- Events whose `(kind, value)` is incompatible with the configured aggregation mode (for example an `Incremental` event arriving at a `Mean`-configured aggregator) are dropped explicitly without affecting the watermark or other in-flight buckets.
- All remaining event-time buckets are flushed when the input stream closes (shutdown or topology reload) so in-flight metrics are never silently dropped.
- In `Diff` mode a small rolling window of previous buckets is retained to compute deltas; other modes do not retain previous buckets.

authors: kaarolch
