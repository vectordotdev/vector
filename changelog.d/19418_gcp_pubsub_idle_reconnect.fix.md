The `gcp_pubsub` source no longer floods the logs with misleading errors on idle, low-volume, or bursty subscriptions. GCP routinely ends a streaming pull with a transient error (such as an `Unavailable` status when there are simply no messages to deliver), which Vector retries automatically. These transient failures are now logged at `debug`, and a `failed_fetching_events` component error is only emitted once several failures occur in a row without any successful fetch in between. The threshold is configurable via the new `max_retry_errors` option (default `5`); a successful fetch resets the counter.

The source is also more resilient to connections that silently break and stop delivering messages until Vector is restarted:

- HTTP/2 keepalive pings are sent on the gRPC connection so a dead-but-open connection is detected and torn down instead of hanging. The ping interval and timeout are configurable via the new `keepalive` option.
- Acknowledgements are sent on a dedicated task, so a backed-up request stream applies backpressure instead of stalling message processing.
- A new `idle_timeout_secs` option (defaulting to `900`) restarts the stream if no activity is seen on an active connection within that window.

authors: SamyDjemai
