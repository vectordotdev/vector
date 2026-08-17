The `kubernetes_logs` source now supports truncating oversized merged log lines instead of
dropping them. A new `max_merged_line_action` configuration option allows choosing between
`drop` (default, existing behavior) and `truncate`. When truncation is enabled, lines exceeding
`max_merged_line_bytes` are truncated to the limit with a `..TRUNCATED` suffix appended.

In `drop` mode, `max_line_bytes` is capped to `max_merged_line_bytes` to avoid wasted I/O.
In `truncate` mode, individual lines up to `max_line_bytes` are allowed through so the merger
can truncate the combined result. Note that `max_line_bytes` still applies at the file level
and always drops individual lines exceeding it; file-level truncation is not yet supported.

authors: vparfonov
