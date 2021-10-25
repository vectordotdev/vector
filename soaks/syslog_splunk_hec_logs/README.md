# Splunk HEC -> Splunk HEC Logs

This soak tests the syslog source feeding directly into the Splunk HEC logs
sink. No transformation is done in the middle so throughput is limited solely by
syslog source's ability to create it, and HEC logs sink's ability to absorb it.

## Method

Lading `tcp_gen` is used to generate syslog load into vector, `http_blackhole`
acts as a Splunk HEC sink.
