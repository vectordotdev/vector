The `websocket` source now correctly associates ping timeouts with connections, preventing premature ping timeout after reconnection. This should prevent stopping Vector in cases when connection with the `websocket` server is unstable.

authors: esensar Quad9DNS
