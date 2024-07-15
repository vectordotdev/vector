Allows for chunked gelf decoding in message-based sources, such as UDP sockets or unix datagram sockets. Implementation is based on [Graylog's documentation](https://go2docs.graylog.org/5-0/getting_in_log_data/gelf.html#GELFviaUDP).

This framing method can be configured via the `framing.method = "chunked_gelf"` option in the source configuration.

authors: jorgehermo9
