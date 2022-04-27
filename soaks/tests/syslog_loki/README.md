# Syslog -> Loki

This soak tests syslog source feeding directly into the loki source. No
transformation is done in the middle so throughput is limited solely by syslog's
ability to create it, loki's ability to absorb it.

## Method

Lading `tcp_gen` is used to generate syslog load into vector, `http_blackhole`
acts as a loki sink.
