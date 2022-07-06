# Syslog -> Humio Logs

This soak tests the syslog source feeding directly into the Humio logs sink.
Throughput is limited solely by syslog source's ability to create it, and Humio
logs sink's ability to absorb it.

## Method

Lading `tcp_gen` is used to generate syslog load into vector, `http_blackhole`
acts as a Humio sink.
