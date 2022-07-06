# Splunk HEC -> Route Transform -> s3

This soak tests the splunk_heck source into the routing transform, dropping into
two s3 sinks. The main area of concern here is the routing transform which is of
high cardinality.

## Method

Lading `http_gen` is used to generate load into vector. `http_blackhole`
mascarades as s3 for the purposes of this soak.
