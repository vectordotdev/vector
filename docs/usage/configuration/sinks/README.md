<!--
     THIS FILE IS AUTOOGENERATED!

     To make changes please edit the template located at:

     scripts/generate/templates/docs/usage/configuration/sinks/README.md.erb
-->

---
description: Receive and pull log and metric events into Vector
---

# Sources

![][images.sinks]

Sinks are last in the [pipeline][docs.pipelines], responsible for sending
[events][docs.event] downstream. These can be service specific sinks, such as
[`vector`][docs.vector_sink], [`elasticsearch`][docs.elasticsearch_sink], and
[`s3`][docs.aws_s3_sink], or generic protocol sinks like
[`http`][docs.http_sink] or [`tcp`][docs.tcp_sink].

| Name  | Description |
|:------|:------------|
| [**`aws_cloudwatch_logs`**][docs.aws_cloudwatch_logs_sink] | Batches [`log`][docs.log_event] events to [AWS CloudWatch Logs][url.aws_cw_logs] via the [`PutLogEvents` API endpoint](https://docs.aws.amazon.com/AmazonCloudWatchLogs/latest/APIReference/API_PutLogEvents.html). |
| [**`aws_kinesis_streams`**][docs.aws_kinesis_streams_sink] | Batches [`log`][docs.log_event] events to [AWS Kinesis Data Stream][url.aws_kinesis_data_streams] via the [`PutRecords` API endpoint](https://docs.aws.amazon.com/kinesis/latest/APIReference/API_PutRecords.html). |
| [**`aws_s3`**][docs.aws_s3_sink] | Batches [`log`][docs.log_event] events to [AWS S3][url.aws_s3] via the [`PutObject` API endpoint](https://docs.aws.amazon.com/AmazonS3/latest/API/RESTObjectPUT.html). |
| [**`blackhole`**][docs.blackhole_sink] | Streams [`log`][docs.log_event] and [`metric`][docs.metric_event] events to a blackhole that simply discards data, designed for testing and benchmarking purposes. |
| [**`console`**][docs.console_sink] | Streams [`log`][docs.log_event] and [`metric`][docs.metric_event] events to the console, `STDOUT` or `STDERR`. |
| [**`elasticsearch`**][docs.elasticsearch_sink] | Batches [`log`][docs.log_event] events to [Elasticsearch][url.elasticsearch] via the [`_bulk` API endpoint](https://www.elastic.co/guide/en/elasticsearch/reference/current/docs-bulk.html). |
| [**`http`**][docs.http_sink] | Batches [`log`][docs.log_event] events to a generic HTTP endpoint. |
| [**`kafka`**][docs.kafka_sink] | Streams [`log`][docs.log_event] events to [Apache Kafka][url.kafka] via the [Kafka protocol][url.kafka_protocol]. |
| [**`prometheus`**][docs.prometheus_sink] | Pulls [`metric`][docs.metric_event] events to [Prometheus][url.prometheus] metrics service. |
| [**`splunk_hec`**][docs.splunk_hec_sink] | Batches [`log`][docs.log_event] events to a [Splunk HTTP Event Collector][url.splunk_hec]. |
| [**`tcp`**][docs.tcp_sink] | Streams [`log`][docs.log_event] events to a TCP connection. |
| [**`vector`**][docs.vector_sink] | Streams [`log`][docs.log_event] events to another downstream Vector instance. |

[+ request a new sink][url.new_sink]


[docs.aws_cloudwatch_logs_sink]: ../../../usage/configuration/sinks/aws_cloudwatch_logs.md
[docs.aws_kinesis_streams_sink]: ../../../usage/configuration/sinks/aws_kinesis_streams.md
[docs.aws_s3_sink]: ../../../usage/configuration/sinks/aws_s3.md
[docs.blackhole_sink]: ../../../usage/configuration/sinks/blackhole.md
[docs.console_sink]: ../../../usage/configuration/sinks/console.md
[docs.elasticsearch_sink]: ../../../usage/configuration/sinks/elasticsearch.md
[docs.event]: ../../../about/data-model.md#event
[docs.http_sink]: ../../../usage/configuration/sinks/http.md
[docs.kafka_sink]: ../../../usage/configuration/sinks/kafka.md
[docs.log_event]: ../../../about/data-model.md#log
[docs.metric_event]: ../../../about/data-model.md#metric
[docs.pipelines]: ../../../usage/configuration/README.md#composition
[docs.prometheus_sink]: ../../../usage/configuration/sinks/prometheus.md
[docs.splunk_hec_sink]: ../../../usage/configuration/sinks/splunk_hec.md
[docs.tcp_sink]: ../../../usage/configuration/sinks/tcp.md
[docs.vector_sink]: ../../../usage/configuration/sinks/vector.md
[images.sinks]: ../../../assets/sinks.svg
[url.aws_cw_logs]: https://docs.aws.amazon.com/AmazonCloudWatch/latest/logs/WhatIsCloudWatchLogs.html
[url.aws_kinesis_data_streams]: https://aws.amazon.com/kinesis/data-streams/
[url.aws_s3]: https://aws.amazon.com/s3/
[url.elasticsearch]: https://www.elastic.co/products/elasticsearch
[url.kafka]: https://kafka.apache.org/
[url.kafka_protocol]: https://kafka.apache.org/protocol
[url.new_sink]: https://github.com/timberio/vector/issues/new?labels=Type%3A+New+Feature
[url.prometheus]: https://prometheus.io/
[url.splunk_hec]: http://dev.splunk.com/view/event-collector/SP-CAAAE6M
