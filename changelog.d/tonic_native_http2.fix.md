The `vector` sink sends configured credentials to HTTP forwarding proxies and applies
`tls.server_name` to TLS connections through CONNECT proxies with its native HTTP/2 client.

The `gcp_pubsub` source can shut down during connection setup and retry delays,
and while an idle stream is open, with acknowledgements enabled or disabled.

The `top` command starts keyboard polling after terminal initialization to prevent
a cursor-position timeout at startup.

authors: thomasqueirozb
