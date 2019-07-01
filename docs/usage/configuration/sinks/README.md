---
description: Receive and pull log and metric events into Vector
---

# Sources

![](../../../assets/sinks.svg)

Sinks are last in the [pipeline][docs.pipelines], responsible for sending [events][docs.event] downstream. These can be service specific sinks, such as [`vector`][docs.vector_sink], [`elasticsearch`][docs.elasticsearch_sink], and [`s3`][docs.aws_s3_sink], or generic protocol sinks like [`http`][docs.http_sink] or [`tcp`][docs.tcp_sink].

<!-- START: sinks_table -->
<!-- ----------------------------------------------------------------- -->
<!-- DO NOT MODIFY! This section is generated via `make generate-docs` -->

| Name | Description |
| :--- | :---------- |
| [**`aws_cloudwatch_logs`**][docs.aws_cloudwatch_logs_sink] | Batches [`log`][docs.log_event] events to [AWS CloudWatch Logs][url.aws_cw_logs] via the [`PutLogEvents` API endpoint](https://docs.aws.amazon.com/AmazonCloudWatchLogs/latest/APIReference/API_PutLogEvents.html). |
| [**`aws_kinesis_streams`**][docs.aws_kinesis_streams_sink] | Batches [`log`][docs.log_event] events to [AWS Kinesis Data Stream][url.aws_kinesis_data_streams] via the [`PutRecords` API endpoint](https://docs.aws.amazon.com/kinesis/latest/APIReference/API_PutRecords.html). |
| [**`aws_s3`**][docs.aws_s3_sink] | Batches [`log`][docs.log_event] events to [AWS S3][url.aws_s3] via the [`PutObject` API endpoint](https://docs.aws.amazon.com/AmazonS3/latest/API/RESTObjectPUT.html). |
| [**`blackhole`**][docs.blackhole_sink] | Streams [`log`][docs.log_event] and [`metric`][docs.metric_event] events to a blackhole that simply discards data, designed for testing and benchmarking purposes. |
| [**`console`**][docs.console_sink] | Streams [`log`][docs.log_event] and [`metric`][docs.metric_event] events to the console, `STDOUT` or `STDERR`. |
| [**`elasticsearch`**][docs.elasticsearch_sink] | Batches [`log`][docs.log_event] events to [Elasticsearch][url.elasticsearch] via the [`_bulk` API endpoint](https://www.elastic.co/guide/en/elasticsearch/reference/current/docs-bulk.html). |
| [**`http`**][docs.http_sink] | Batches [`log`][docs.log_event] events to a generic HTTP endpoint. |
| [**`kafka`**][docs.kafka_sink] | Streams [`log`][docs.log_event] events to [Apache Kafka][url.kafka] via the [Kafka protocol][url.kafka_protocol]. |
| [**`splunk_hec`**][docs.splunk_hec_sink] | Batches [`log`][docs.log_event] events to a [Splunk HTTP Event Collector][url.splunk_hec]. |
| [**`tcp`**][docs.tcp_sink] | Streams [`log`][docs.log_event] events to a TCP connection. |
| [**`vector`**][docs.vector_sink] | Streams [`log`][docs.log_event] events to another downstream Vector instance. |

<!-- ----------------------------------------------------------------- -->
<!-- END: sinks_table -->

[+ request a new transform](https://github.com/timberio/vector/issues/new?labels=Type%3A+New+Feature%2C%7B%3Atitle%3D%3E%22New+%60%3Cname%3E%60+sink%22%7D&title=New+%60%3Cname%3E%60+sink)

## How It Works

Sinks are responsible for forwarding [events][docs.event] downstream. They generally overlap in behavior falling into 2 categories: streaming or batching. To provide high-level structure we'll cover the common behavioral traits here to establish an understanding of shared behavior. For explicitness, each sink will document this behavior as well.

### Buffers vs. Batches

For sinks that batch and flush it's helpful to understand the difference between buffers and batches within Vector. Batches represent the batched payload being sent to the downstream service while buffers represent the internal data buffer Vector uses for each sink. More detailed descriptions are as follows.

#### Batches

Batches represent the batched payload being sent to the downstream service. Sinks will provide 2 options to control the size and age before being sent, the `batch_size` and `batch_timeout` options. They will be documented in a "Batching" section within any sink that supports them.

### Healthchecks

All sinks are required to implement a healthcheck behavior. This is intended to be a light weight check to ensure downstream availability and avoid subsequent failures if the service is not available. Additionally, you can require all health checks to pass via the `--require-healthy` flag when [starting][docs.starting] Vector.

### Rate Limiting

Any sink that batches will include options to rate limit requests. These options include the `request_in_flight_limit`, `request_timeout_secs`, and `request_rate_limit_duration_secs`, `request_rate_limit_num`. For explicitness, these options will be documented directly on the sinks that support them.

### Retries

Any sink that batches will include options to retry failed requests. These options include the `request_retry_attempts` , and `request_retry_backoff_secs`. For explicitness, these options will be documented directly on the sinks that support them.

### Timeouts

All sinks will support a `request_timeout_secs` option. This will kill long running requests. It's highly recommended that you configure timeouts downstream to be less than the value here. This will ensure Vector does not pile on requests.

### Vector to Vector Communication

If you're sending data to another downstream [Vector service][docs.service_role] then you should use the [`vector` sink][docs.vector_sink], with the downstream service using the [`vector` source][docs.vector_source].

{% page-ref page="../../guides/vector-to-vector-guide.md" %}


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
[docs.service_role]: ../../../setup/deployment/roles/service.md
[docs.splunk_hec_sink]: ../../../usage/configuration/sinks/splunk_hec.md
[docs.starting]: ../../../usage/administration/starting.md
[docs.tcp_sink]: ../../../usage/configuration/sinks/tcp.md
[docs.vector_sink]: ../../../usage/configuration/sinks/vector.md
[docs.vector_source]: ../../../usage/configuration/sources/vector.md
[url.aws_cw_logs]: https://docs.aws.amazon.com/AmazonCloudWatch/latest/logs/WhatIsCloudWatchLogs.html
[url.aws_kinesis_data_streams]: https://aws.amazon.com/kinesis/data-streams/
[url.aws_s3]: https://aws.amazon.com/s3/
[url.elasticsearch]: https://www.elastic.co/products/elasticsearch
[url.kafka]: https://kafka.apache.org/
[url.kafka_protocol]: https://kafka.apache.org/protocol
[url.splunk_hec]: http://dev.splunk.com/view/event-collector/SP-CAAAE6M
