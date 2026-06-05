Fixed a panic in the statsd source gauge parser triggered by a multi-byte UTF-8 character in the metric value position. A single malformed UDP datagram could crash the entire Vector process. The invalid value now returns a parse error instead.

authors: pront
