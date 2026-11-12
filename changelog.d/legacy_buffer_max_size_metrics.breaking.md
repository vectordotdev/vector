# Legacy buffer maximum-size metrics removed {#legacy-buffer-max-size-metrics-removed}

## Summary

The deprecated `buffer_max_byte_size`, `buffer_max_event_size`, `source_buffer_max_byte_size`,
`source_buffer_max_event_size`, `transform_buffer_max_byte_size`, and
`transform_buffer_max_event_size` gauge metrics have been removed. They were deprecated in 0.53.0
in favor of the `*_max_size_bytes` and `*_max_size_events` gauges, which report the same values.

## Migration

Update dashboards and alerts to use the new names:

- `buffer_max_byte_size` → `buffer_max_size_bytes`
- `buffer_max_event_size` → `buffer_max_size_events`
- `source_buffer_max_byte_size` → `source_buffer_max_size_bytes`
- `source_buffer_max_event_size` → `source_buffer_max_size_events`
- `transform_buffer_max_byte_size` → `transform_buffer_max_size_bytes`
- `transform_buffer_max_event_size` → `transform_buffer_max_size_events`

authors: JinchengCao
