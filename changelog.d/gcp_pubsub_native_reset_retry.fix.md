The `gcp_pubsub` source again reconnects immediately, without logging an error, when the server resets a streaming pull mid-stream; these resets were previously retried only after `retry_delay_secs`. When `tls.ca_file` contains several certificates, all of them are now trusted in addition to the system roots, instead of only the last one.

authors: thomasqueirozb
