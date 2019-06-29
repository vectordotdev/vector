---
description: An in-depth look into Vector's delivery guarantees
---

# Guarantees

Vector was designed with a focus on providing clear guarantees. Below you'll
find a support matrix so you know exactly what type of guarantee you can expect
for your combination of sources and sinks. This helps you make the appropriate
tradeoffs or your usecase.

## Support Matrix

The following matrix outlines the guarantee support for each [sink][docs.sinks]
and [source][docs.sources].

<!-- START: support_matrix_table -->
<!-- ----------------------------------------------------------------- -->
<!-- DO NOT MODIFY! This section is generated via `make generate-docs` -->

| Name | Description |
| :--- | :---------- |
| [`aws_cloudwatch_logs` sink][docs.aws_cloudwatch_logs_sink] | `at_least_once` |
| [`aws_kinesis_streams` sink][docs.aws_kinesis_streams_sink] | `at_least_once` |
| [`aws_s3` sink][docs.aws_s3_sink] | `at_least_once` |
| [`blackhole` sink][docs.blackhole_sink] | `best_effort` |
| [`console` sink][docs.console_sink] | `best_effort` |
| [`elasticsearch` sink][docs.elasticsearch_sink] | `best_effort` |
| [`file` source][docs.file_source] | `best_effort` |
| [`http` sink][docs.http_sink] | `at_least_once` |
| [`kafka` sink][docs.kafka_sink] | `at_least_once` |
| [`splunk_hec` sink][docs.splunk_hec_sink] | `at_least_once` |
| [`statsd` source][docs.statsd_source] | `best_effort` |
| [`stdin` source][docs.stdin_source] | `at_least_once` |
| [`syslog` source][docs.syslog_source] | `best_effort` |
| [`tcp` sink][docs.tcp_sink] | `best_effort` |
| [`tcp` source][docs.tcp_source] | `best_effort` |
| [`vector` sink][docs.vector_sink] | `best_effort` |
| [`vector` source][docs.vector_source] | `best_effort` |

<!-- ----------------------------------------------------------------- -->
<!-- END: support_matrix_table -->

## At Least Once Delivery

At least once delivery guarantees that an [event][docs.event] received by
Vector will be delivered at least once to the configured destination(s). While
rare, it is possible for an event to be delivered more than once (see the
[Does Vector support exactly once delivery](#does-vector-support-exactly-once-delivery) FAQ below).

## Best Effort Delivery

Best effort delivery has no guarantees and means that Vector will make a best
effort to deliver each event. This means it is possible for an event to not be
delivered. For most, this is sufficient in the observability use case and will
afford you the opportunity to optimize towards performance and reduce operating
cost. For example, you can stick with in-memory buffers (default), instead of
enabling on-disk buffers to improve performance.

## FAQs

### Do I need at least once delivery?

One of the unique advantages with the logging use case is that some data loss
is usually acceptable. This is due to the fact that log data is usually used
for diagnostic purposes and losing an event has little impact on the business.
This is not to say that Vector does not take the at least once guarantee very
seriously, it just means that you can optimize towards performance and reduce
your cost if you're willing to accept some data loss.

### Does Vector support exactly once delivery?

No, Vector does not support exactly once delivery. There are future plans to
partially support this for sources and sinks that support it (Kafka, for
example), but it remains unclear if Vector will ever be able to achieve this.
We recommend [subscribing to our mailing list](https://vector.dev), which will
keep you in the loop if this ever changes.


[docs.aws_cloudwatch_logs_sink]: ../usage/configuration/sinks/aws_cloudwatch_logs.md
[docs.aws_kinesis_streams_sink]: ../usage/configuration/sinks/aws_kinesis_streams.md
[docs.aws_s3_sink]: ../usage/configuration/sinks/aws_s3.md
[docs.blackhole_sink]: ../usage/configuration/sinks/blackhole.md
[docs.console_sink]: ../usage/configuration/sinks/console.md
[docs.elasticsearch_sink]: ../usage/configuration/sinks/elasticsearch.md
[docs.event]: ../about/data-model.md#event
[docs.file_source]: ../usage/configuration/sources/file.md
[docs.http_sink]: ../usage/configuration/sinks/http.md
[docs.kafka_sink]: ../usage/configuration/sinks/kafka.md
[docs.sinks]: ../usage/configuration/sinks
[docs.sources]: ../usage/configuration/sources
[docs.splunk_hec_sink]: ../usage/configuration/sinks/splunk_hec.md
[docs.statsd_source]: ../usage/configuration/sources/statsd.md
[docs.stdin_source]: ../usage/configuration/sources/stdin.md
[docs.syslog_source]: ../usage/configuration/sources/syslog.md
[docs.tcp_sink]: ../usage/configuration/sinks/tcp.md
[docs.tcp_source]: ../usage/configuration/sources/tcp.md
[docs.vector_sink]: ../usage/configuration/sinks/vector.md
[docs.vector_source]: ../usage/configuration/sources/vector.md
