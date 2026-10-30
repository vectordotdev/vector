Added `replay` configuration option to `websocket` source, making it possible to receive missed
messages between connections from supported server (like `websocket_server` sink with
`message_buffering` enabled). Custom `headers` can now also be defined on `websocket` source, to be
added to the connection request, which can be useful in this combination to be identified by the
server side.

authors: esensar Quad9DNS
