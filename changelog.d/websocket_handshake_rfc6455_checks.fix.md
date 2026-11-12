The `websocket` source and `websocket` sink now fail the handshake when the server returns a `Sec-WebSocket-Protocol` header, because these components do not request a subprotocol (RFC 6455). Before, the header was ignored. The `websocket_server` sink now rejects clients that send a `Sec-WebSocket-Key` that is not a base64-encoded 16-byte value.

authors: thomasqueirozb
