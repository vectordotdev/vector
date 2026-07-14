The `fluent` source now caps how large a single msgpack frame may grow while being buffered, using the same limit, so a peer can no longer stream an oversized array/map/string without ever completing a message. Frames that exceed the limit before a complete message is decoded are now rejected and the connection is closed.

authors: thomasqueirozb
