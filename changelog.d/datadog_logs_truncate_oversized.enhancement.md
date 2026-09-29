The `datadog_logs` sink can now optionally truncate logs that exceed Datadog's
per-log size limit. Configure `truncate_oversized_logs` to set the encoded log
and retained message limits, mark shortened messages, tag reduced logs, and
preserve standard Datadog fields. Logs that cannot be reduced below the
configured limit are dropped.

authors: bruceg
