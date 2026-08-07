The `character_delimited` and `newline_delimited` decoding framers now return an error instead of silently logging a `warn!` when discarding a frame that exceeds `max_length`. This surfaces the drop through the standard `component_errors_total` metric (`error_code="decoder_frame"`) so it is observable even when trace-level logs are suppressed, instead of being invisible. The connection is not affected — the offending frame is still discarded and processing continues.

authors: lisaqvu
