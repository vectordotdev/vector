# Legacy buffer metrics removed {#legacy-buffer-metrics-removed}

## Summary

The deprecated `buffer_byte_size` and `buffer_events` gauge metrics have been removed.

## Migration

Use `buffer_size_bytes` instead of `buffer_byte_size` and `buffer_size_events`
instead of `buffer_events`.

authors: pront
