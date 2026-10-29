---
what: "`buffer_max_byte_size` and `buffer_max_event_size` gauge metrics"
deprecated_since: "0.53.0"
---

The buffer maximum-size gauges were renamed to follow Vector's metric naming standard. The legacy
names are still emitted, but they are deprecated and will be removed in a future release.

Migrate any dashboards or alerts that reference:

- `buffer_max_byte_size`
- `buffer_max_event_size`
- `source_buffer_max_byte_size`
- `source_buffer_max_event_size`
- `transform_buffer_max_byte_size`
- `transform_buffer_max_event_size`

to:

- `buffer_max_size_bytes`
- `buffer_max_size_events`
- `source_buffer_max_size_bytes`
- `source_buffer_max_size_events`
- `transform_buffer_max_size_bytes`
- `transform_buffer_max_size_events`
