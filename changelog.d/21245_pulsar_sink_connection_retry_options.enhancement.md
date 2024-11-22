Expose `connection_retry_options` in the Pulsar sink configuration to allow customizing the connection retry behaviour of the pulsar client. This includes the following options:

- `min_backoff_ms`: Minimum delay between connection retries.
- `max_backoff_secs`: Maximum delay between reconnection retries.
- `max_retries`: Maximum number of connection retries.
- `connection_timeout_secs`: Time limit to establish a connection.
- `keep_alive_secs`: Keep-alive interval for each broker connection.

authors: FRosner
