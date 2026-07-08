Fixed a potential panic in the `statsd` source when a gauge metric value begins with a multi-byte UTF-8 character. The invalid value now returns a parse error instead.

authors: pront
