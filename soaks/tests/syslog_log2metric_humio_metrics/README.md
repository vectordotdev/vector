# Syslog -> VRL (parse_syslog) -> Log2Metric -> Humio Metrics

This soak tests the syslog source feeding into the Humio metrics sink after the
syslog events are parsed and converted to metrics. Throughput may be limited by
the associated transforms.

## Method

Lading `tcp_gen` is used to generate syslog load into vector, `http_blackhole`
acts as a Humio sink.
