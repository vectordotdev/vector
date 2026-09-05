The `gcp_pubsub` source no longer logs an ERROR or increments `component_errors_total`
when the server closes a StreamingPull stream for an expected reason. These routine
closures are now logged at debug level and trigger an immediate reconnect instead of
waiting for `retry_delay_secs`.

authors: andylibrian
