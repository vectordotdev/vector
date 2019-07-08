---
description: An in-depth look into Vector's delivery guarantees
---

<!--
     THIS FILE IS AUTOOGENERATED!

     To make changes please edit the template located at:

     scripts/generate/templates/docs/about/guarantees.md.erb
-->

# Guarantees

Vector was designed with a focus on providing clear guarantees. Below you'll
find a support matrix so you know exactly what type of guarantee you can expect
for your combination of sources and sinks. This helps you make the appropriate
tradeoffs or your usecase.

## Support Matrix

The following matrix outlines the guarantee support for each [sink][docs.sinks]
and [source][docs.sources].

### Sources

| Name | Description |
| :--- | :---------- |

| [`file` source][docs.file_source] | `best_effort` |

| [`statsd` source][docs.statsd_source] | `best_effort` |

| [`stdin` source][docs.stdin_source] | `at_least_once` |

| [`syslog` source][docs.syslog_source] | `best_effort` |

| [`tcp` source][docs.tcp_source] | `best_effort` |

| [`vector` source][docs.vector_source] | `best_effort` |


### Sinks

| Name | Description |
| :--- | :---------- |

| [`aws_cloudwatch_logs` sink][docs.aws_cloudwatch_logs_sink] | `at_least_once` |

| [`aws_kinesis_streams` sink][docs.aws_kinesis_streams_sink] | `at_least_once` |

| [`aws_s3` sink][docs.aws_s3_sink] | `at_least_once` |

| [`blackhole` sink][docs.blackhole_sink] | `best_effort` |

| [`console` sink][docs.console_sink] | `best_effort` |

| [`elasticsearch` sink][docs.elasticsearch_sink] | `best_effort` |

| [`http` sink][docs.http_sink] | `at_least_once` |

| [`kafka` sink][docs.kafka_sink] | `at_least_once` |

| [`prometheus` sink][docs.prometheus_sink] | `at_least_once` |

| [`splunk_hec` sink][docs.splunk_hec_sink] | `at_least_once` |

| [`tcp` sink][docs.tcp_sink] | `best_effort` |

| [`vector` sink][docs.vector_sink] | `best_effort` |


## At Least Once Delivery

"At least once" delivery guarantees that an [event][docs.event] received by
Vector will be delivered at least once to the configured destination(s). While
rare, it is possible for an event to be delivered more than once (see the
[Does Vector support exactly once delivery](#does-vector-support-exactly-once-delivery)
FAQ below).

## Best Effort Delivery

"Best effort" delivery has no guarantees and means that Vector will make a best
effort to deliver each event. This means it is possible for the occassional
event to not be lost.

## FAQs

### Do I need at least once delivery?

One of the unique advantages of the logging use case is that data is usually
used for diagnostic purposes only. Therefore, losing the occassional event
has little impact on your business. This affords you the opportunity to
provision your pipeline towards performance, simplicity, and cost reduction.

On the hand, if you're using your data to perform business critical functions,
then data loss is not acceptable and therefore requires "at least once" deliery.

To clarify, even though a source or sink is marked as "best effort" it does
not mean Vector takes delivery lightly. In fact, once data is within the
boundary of Vector it will not be lost if you've configured on-disk buffers.
Data loss for "best effort" sources and sinks are almost always due to the
limitations of the underlying protocol.

### Does Vector support exactly once delivery?

No, Vector does not support exactly once delivery. There are future plans to
partially support this for sources and sinks that support it (Kafka, for
example), but it remains unclear if Vector will ever be able to achieve this.
We recommend [subscribing to our mailing list](https://vector.dev), which will
keep you in the loop if this ever changes.


[docs.aws_cloudwatch_logs_sink]: https://docs.vector.dev/usage/configuration/sinks/aws_cloudwatch_logs
[docs.aws_kinesis_streams_sink]: https://docs.vector.dev/usage/configuration/sinks/aws_kinesis_streams
[docs.aws_s3_sink]: https://docs.vector.dev/usage/configuration/sinks/aws_s3
[docs.blackhole_sink]: https://docs.vector.dev/usage/configuration/sinks/blackhole
[docs.console_sink]: https://docs.vector.dev/usage/configuration/sinks/console
[docs.elasticsearch_sink]: https://docs.vector.dev/usage/configuration/sinks/elasticsearch
[docs.event]: https://docs.vector.dev/about/data-model#event
[docs.file_source]: https://docs.vector.dev/usage/configuration/sources/file
[docs.http_sink]: https://docs.vector.dev/usage/configuration/sinks/http
[docs.kafka_sink]: https://docs.vector.dev/usage/configuration/sinks/kafka
[docs.prometheus_sink]: https://docs.vector.dev/usage/configuration/sinks/prometheus
[docs.sinks]: https://docs.vector.dev/usage/configuration/sinks
[docs.sources]: https://docs.vector.dev/usage/configuration/sources
[docs.splunk_hec_sink]: https://docs.vector.dev/usage/configuration/sinks/splunk_hec
[docs.statsd_source]: https://docs.vector.dev/usage/configuration/sources/statsd
[docs.stdin_source]: https://docs.vector.dev/usage/configuration/sources/stdin
[docs.syslog_source]: https://docs.vector.dev/usage/configuration/sources/syslog
[docs.tcp_sink]: https://docs.vector.dev/usage/configuration/sinks/tcp
[docs.tcp_source]: https://docs.vector.dev/usage/configuration/sources/tcp
[docs.vector_sink]: https://docs.vector.dev/usage/configuration/sinks/vector
[docs.vector_source]: https://docs.vector.dev/usage/configuration/sources/vector
