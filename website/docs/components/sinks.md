---
title: Sinks
sidebar_label: hidden
---

Sinks are last in the [pipeline][docs.configuration#composition], responsible
for sending [events][docs.data-model#event] downstream.

import Component from '@site/src/components/Component';
import Components from '@site/src/components/Components';

<Components>

<Component
  delivery_guarantee={"at_least_once"}
  description={"Batches `log` events to AWS CloudWatch Logs via the `PutLogEvents` API endpoint."}
  event_types={["log"]}
  id={"aws_cloudwatch_logs_sink"}
  name={"aws_cloudwatch_logs"}
  path={"/components/sinks/aws_cloudwatch_logs"}
  status={"beta"}
  type={"sink"} />
<Component
  delivery_guarantee={"at_least_once"}
  description={"Streams `metric` events to AWS CloudWatch Metrics via the `PutMetricData` API endpoint."}
  event_types={["metric"]}
  id={"aws_cloudwatch_metrics_sink"}
  name={"aws_cloudwatch_metrics"}
  path={"/components/sinks/aws_cloudwatch_metrics"}
  status={"beta"}
  type={"sink"} />
<Component
  delivery_guarantee={"at_least_once"}
  description={"Batches `log` events to AWS Kinesis Data Stream via the `PutRecords` API endpoint."}
  event_types={["log"]}
  id={"aws_kinesis_streams_sink"}
  name={"aws_kinesis_streams"}
  path={"/components/sinks/aws_kinesis_streams"}
  status={"beta"}
  type={"sink"} />
<Component
  delivery_guarantee={"at_least_once"}
  description={"Batches `log` events to AWS S3 via the `PutObject` API endpoint."}
  event_types={["log"]}
  id={"aws_s3_sink"}
  name={"aws_s3"}
  path={"/components/sinks/aws_s3"}
  status={"beta"}
  type={"sink"} />
<Component
  delivery_guarantee={"best_effort"}
  description={"Streams `log` and `metric` events to a blackhole that simply discards data, designed for testing and benchmarking purposes."}
  event_types={["log","metric"]}
  id={"blackhole_sink"}
  name={"blackhole"}
  path={"/components/sinks/blackhole"}
  status={"prod-ready"}
  type={"sink"} />
<Component
  delivery_guarantee={"best_effort"}
  description={"Batches `log` events to Clickhouse via the `HTTP` Interface."}
  event_types={["log"]}
  id={"clickhouse_sink"}
  name={"clickhouse"}
  path={"/components/sinks/clickhouse"}
  status={"beta"}
  type={"sink"} />
<Component
  delivery_guarantee={"best_effort"}
  description={"Streams `log` and `metric` events to standard output streams, such as `STDOUT` and `STDERR`."}
  event_types={["log","metric"]}
  id={"console_sink"}
  name={"console"}
  path={"/components/sinks/console"}
  status={"prod-ready"}
  type={"sink"} />
<Component
  delivery_guarantee={"best_effort"}
  description={"Batches `metric` events to Datadog metrics service using HTTP API."}
  event_types={["metric"]}
  id={"datadog_metrics_sink"}
  name={"datadog_metrics"}
  path={"/components/sinks/datadog_metrics"}
  status={"beta"}
  type={"sink"} />
<Component
  delivery_guarantee={"best_effort"}
  description={"Batches `log` events to Elasticsearch via the `_bulk` API endpoint."}
  event_types={["log"]}
  id={"elasticsearch_sink"}
  name={"elasticsearch"}
  path={"/components/sinks/elasticsearch"}
  status={"beta"}
  type={"sink"} />
<Component
  delivery_guarantee={"best_effort"}
  description={"Streams `log` events to a file."}
  event_types={["log"]}
  id={"file_sink"}
  name={"file"}
  path={"/components/sinks/file"}
  status={"prod-ready"}
  type={"sink"} />
<Component
  delivery_guarantee={"at_least_once"}
  description={"Batches `log` events to a generic HTTP endpoint."}
  event_types={["log"]}
  id={"http_sink"}
  name={"http"}
  path={"/components/sinks/http"}
  status={"prod-ready"}
  type={"sink"} />
<Component
  delivery_guarantee={"at_least_once"}
  description={"Streams `log` events to Apache Kafka via the Kafka protocol."}
  event_types={["log"]}
  id={"kafka_sink"}
  name={"kafka"}
  path={"/components/sinks/kafka"}
  status={"prod-ready"}
  type={"sink"} />
<Component
  delivery_guarantee={"best_effort"}
  description={"Exposes `metric` events to Prometheus metrics service."}
  event_types={["metric"]}
  id={"prometheus_sink"}
  name={"prometheus"}
  path={"/components/sinks/prometheus"}
  status={"beta"}
  type={"sink"} />
<Component
  delivery_guarantee={"at_least_once"}
  description={"Batches `log` events to a Splunk HTTP Event Collector."}
  event_types={["log"]}
  id={"splunk_hec_sink"}
  name={"splunk_hec"}
  path={"/components/sinks/splunk_hec"}
  status={"prod-ready"}
  type={"sink"} />
<Component
  delivery_guarantee={"best_effort"}
  description={"Streams `metric` events to StatsD metrics service."}
  event_types={["metric"]}
  id={"statsd_sink"}
  name={"statsd"}
  path={"/components/sinks/statsd"}
  status={"beta"}
  type={"sink"} />
<Component
  delivery_guarantee={"best_effort"}
  description={"Streams `log` events to a TCP connection."}
  event_types={["log"]}
  id={"tcp_sink"}
  name={"tcp"}
  path={"/components/sinks/tcp"}
  status={"prod-ready"}
  type={"sink"} />
<Component
  delivery_guarantee={"best_effort"}
  description={"Streams `log` events to another downstream Vector instance."}
  event_types={["log"]}
  id={"vector_sink"}
  name={"vector"}
  path={"/components/sinks/vector"}
  status={"prod-ready"}
  type={"sink"} />

</Components>


[+ request a new sink][urls.new_sink]


[docs.configuration#composition]: ../setup/configuration#composition
[docs.data-model#event]: ../about/data-model#event
[urls.new_sink]: https://github.com/timberio/vector/issues/new?labels=Type%3A+New+Feature
