HTTP client based components, such as the `http` and `prometheus_remote_write` sinks, now limit connection setup (the TCP connect, the TLS handshake and a proxy `CONNECT` tunnel) to 65 seconds, and outbound HTTP connections now enable TCP keepalive. Previously, a connection that the connection pool kept setting up in the background could stall in the TLS handshake forever, for example on a lossy network, and the leaked sockets eventually exhausted the process's file descriptors.

authors: AxisNow-Dev
