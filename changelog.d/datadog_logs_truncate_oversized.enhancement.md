The `datadog_logs` sink can now optionally truncate logs that exceed Datadog's
per-log size limit. Configure `truncate_oversized_logs` to set the encoded log
limit, mark shortened messages, and tag reduced logs. Logs whose non-message
fields leave no room for a truncated message are forwarded unchanged when they
fit the payload limit, allowing the Datadog intake to truncate them. Logs
without a string message to truncate are dropped.

authors: bruceg
