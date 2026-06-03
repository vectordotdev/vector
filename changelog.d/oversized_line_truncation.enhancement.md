The `kubernetes_logs` source now supports truncating oversized merged log lines instead of dropping them.
A new `max_merged_line_action` configuration option allows choosing between `drop` (default, existing behavior)
and `truncate` (new). When truncation is enabled, lines exceeding `max_merged_line_bytes` are truncated to the limit
and appended with a `..TRUNCATED` suffix to indicate the message is incomplete.

Also adds `OversizedAction` enum to the framing decoders (`CharacterDelimitedDecoder`, `NewlineDelimitedDecoder`)
to support truncation at the frame level in addition to the merged-line level.

Issue: https://github.com/vectordotdev/vector/pull/22582

authors: vparfonov
