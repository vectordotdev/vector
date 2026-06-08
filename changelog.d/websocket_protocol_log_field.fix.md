Fixed a typo in the `WebSocketMessageReceived` internal event emitted by the `websocket` source: the `protocol` field was previously misspelled as `protcol`. Users filtering on this field in trace-level logs should update their queries accordingly.

authors: pront
