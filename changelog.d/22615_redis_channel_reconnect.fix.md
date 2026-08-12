The `redis` source configured with `data_type = "channel"` now automatically reconnects and re-subscribes after the Redis connection drops (for example on a Redis restart or a transient network blip), instead of silently stopping until Vector is restarted. Reconnect attempts use exponential backoff (capped at 30s) and emit `component_errors_total` on failures and `connection_established_total` on recovery.

authors: gibranbadrul
