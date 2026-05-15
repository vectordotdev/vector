Added a new `iggy` sink that publishes observability data to a topic on the
[Iggy](https://iggy.apache.org) message streaming platform. It supports the TCP,
QUIC, HTTP, and WebSocket transports via Iggy's connection-string format and
creates the configured stream and topic on connect when they do not already
exist.

authors: jpds
