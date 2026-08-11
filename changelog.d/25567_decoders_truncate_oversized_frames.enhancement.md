The `character_delimited` and `newline_delimited` decoders now support truncating oversized frames.
A new `oversized_action`configuration option allows choosing between `drop` (default, existing behavior) and `truncate`.
When `oversized_action` is set to `truncate`, frames that exceed the configured `max_length` are truncated to the maximum 
allowed size, and the remainder of the oversized frame is discarded up to the next delimiter.

authors: vparfonov