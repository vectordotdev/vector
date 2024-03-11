We now correctly calculate the estimated JSON size in bytes for the metric `component_received_event_bytes_total` for the `splunk_hec` source.

Previously this was being calculated after event enrichment. It is now calculated before enrichment, for both `raw` and `event` endpoints.
