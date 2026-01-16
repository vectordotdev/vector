Buffers now emit metric names for sizes that better follow the metric naming standard specification
while keeping the old related gauges available for a transition period. Operators should update
dashboards/alerts to the new variants as the legacy names are now deprecated.

* `buffer_max_size_bytes` deprecates `buffer_max_byte_size`
* `buffer_max_size_events` deprecates `buffer_max_event_size`
* `buffer_size_bytes` deprecates `buffer_byte_size`
* `buffer_size_events` deprecates `buffer_events`

authors: bruceg
