AWS components now send the configured proxy credentials to an HTTP forward proxy for plain `http://` AWS endpoints, for example a custom `endpoint` or the EC2 instance metadata service. Before, these requests reached the proxy without a `Proxy-Authorization` header. HTTPS requests through a `CONNECT` proxy are unchanged.

authors: thomasqueirozb
