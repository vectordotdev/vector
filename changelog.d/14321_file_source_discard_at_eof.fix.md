The `file` and `kubernetes_logs` sources now report a line that exceeds `max_line_bytes` when the read that discards it reaches the end of the file. Previously, `component_errors_total`, `component_discarded_events_total` and the error log were skipped in that case. An oversized line without a delimiter yet is reported once rather than on every read.

authors: vivek-raj1
