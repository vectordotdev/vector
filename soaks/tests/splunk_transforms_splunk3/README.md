# Splunk -> Transforms -> Multiple Splunk

This soak tests the splunk source feeding into a small collection of transforms
that then route out into multiple syslog sinks.

## Method

Lading `http_gen` is used to generate splunk load into vector, `http_blackhole`
acts as a Splunk HEC sink.
