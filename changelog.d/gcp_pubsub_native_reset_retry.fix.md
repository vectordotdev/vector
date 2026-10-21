The `gcp_pubsub` source again reconnects immediately when the server resets a streaming pull,
instead of after `retry_delay_secs`. It also trusts every certificate in `tls.ca_file`, not only
the last one.

authors: thomasqueirozb
