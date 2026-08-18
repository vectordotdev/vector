Prevent the `datadog_traces` sink from starting its APM statistics flusher while the sink is being
built. The flusher now follows the sink lifecycle, avoiding leaked background tasks during
configuration validation or a rolled-back reload. Shutdown waits only a bounded time for the final
APM statistics flush when the endpoint is unreachable.

authors: kurochan
