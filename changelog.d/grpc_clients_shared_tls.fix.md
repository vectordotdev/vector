The `gcp_pubsub` source now uses Vector's shared TLS and proxy settings. It honors
`tls.verify_certificate`, `tls.verify_hostname`, `tls.server_name`, and `tls.alpn_protocols`,
and routes requests through the configured proxy. The default ALPN protocol remains `h2`.
The source and the `vector top` and `vector tap` API clients now use Vector's OpenSSL trust store.

Connection failures still use `retry_delay_secs`; server stream resets still retry immediately.

authors: thomasqueirozb
