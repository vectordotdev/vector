The `gcp_pubsub` source now reconnects immediately, without logging an error, when the server
resets a streaming pull. Since Vector 0.35.0 it logged an error and waited `retry_delay_secs`.
It also trusts every certificate in `tls.ca_file`, not only the last one.

authors: thomasqueirozb
