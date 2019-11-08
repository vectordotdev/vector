---
title: Components
sidebar_label: hidden
hide_pagination: true
---

Vector components are the entities used to
[compose pipelines][docs.configuration#composition].

import Components from '@site/src/components/Components';

import Component from '@site/src/components/Component';

<Components>

<Component
  delivery_guarantee={"best_effort"}
  description={"Ingests data through the docker engine daemon and outputs `log` events."}
  event_types={["log"]}
  id={"docker_source"}
  name={"docker"}
  path="[[docs.#{component.type.pluralize}.#{component.name}]]"
  status={"beta"}
  type={"source"} />
<Component
  delivery_guarantee={"best_effort"}
  description={"Ingests data through one or more local files and outputs `log` events."}
  event_types={["log"]}
  id={"file_source"}
  name={"file"}
  path="[[docs.#{component.type.pluralize}.#{component.name}]]"
  status={"prod-ready"}
  type={"source"} />
<Component
  delivery_guarantee={"best_effort"}
  description={"Ingests data through log records from journald and outputs `log` events."}
  event_types={["log"]}
  id={"journald_source"}
  name={"journald"}
  path="[[docs.#{component.type.pluralize}.#{component.name}]]"
  status={"beta"}
  type={"source"} />
<Component
  delivery_guarantee={"at_least_once"}
  description={"Ingests data through Kafka 0.9 or later and outputs `log` events."}
  event_types={["log"]}
  id={"kafka_source"}
  name={"kafka"}
  path="[[docs.#{component.type.pluralize}.#{component.name}]]"
  status={"beta"}
  type={"source"} />
<Component
  delivery_guarantee={"best_effort"}
  description={"Ingests data through the StatsD UDP protocol and outputs `metric` events."}
  event_types={["metric"]}
  id={"statsd_source"}
  name={"statsd"}
  path="[[docs.#{component.type.pluralize}.#{component.name}]]"
  status={"beta"}
  type={"source"} />
<Component
  delivery_guarantee={"at_least_once"}
  description={"Ingests data through standard input (STDIN) and outputs `log` events."}
  event_types={["log"]}
  id={"stdin_source"}
  name={"stdin"}
  path="[[docs.#{component.type.pluralize}.#{component.name}]]"
  status={"prod-ready"}
  type={"source"} />
<Component
  delivery_guarantee={"best_effort"}
  description={"Ingests data through the Syslog 5424 protocol and outputs `log` events."}
  event_types={["log"]}
  id={"syslog_source"}
  name={"syslog"}
  path="[[docs.#{component.type.pluralize}.#{component.name}]]"
  status={"prod-ready"}
  type={"source"} />
<Component
  delivery_guarantee={"best_effort"}
  description={"Ingests data through the TCP protocol and outputs `log` events."}
  event_types={["log"]}
  id={"tcp_source"}
  name={"tcp"}
  path="[[docs.#{component.type.pluralize}.#{component.name}]]"
  status={"prod-ready"}
  type={"source"} />
<Component
  delivery_guarantee={"best_effort"}
  description={"Ingests data through the UDP protocol and outputs `log` events."}
  event_types={["log"]}
  id={"udp_source"}
  name={"udp"}
  path="[[docs.#{component.type.pluralize}.#{component.name}]]"
  status={"prod-ready"}
  type={"source"} />
<Component
  delivery_guarantee={"best_effort"}
  description={"Ingests data through another upstream Vector instance and outputs `log` and `metric` events."}
  event_types={["log","metric"]}
  id={"vector_source"}
  name={"vector"}
  path="[[docs.#{component.type.pluralize}.#{component.name}]]"
  status={"beta"}
  type={"source"} />
<Component
  delivery_guarantee={null}
  description={"Accepts `log` events and allows you to add one or more log fields."}
  event_types={["log"]}
  id={"add_fields_transform"}
  name={"add_fields"}
  path="[[docs.#{component.type.pluralize}.#{component.name}]]"
  status={"prod-ready"}
  type={"transform"} />
<Component
  delivery_guarantee={null}
  description={"Accepts `metric` events and allows you to add one or more metric tags."}
  event_types={["metric"]}
  id={"add_tags_transform"}
  name={"add_tags"}
  path="[[docs.#{component.type.pluralize}.#{component.name}]]"
  status={"prod-ready"}
  type={"transform"} />
<Component
  delivery_guarantee={null}
  description={"Accepts `log` events and allows you to coerce log fields into fixed types."}
  event_types={["log"]}
  id={"coercer_transform"}
  name={"coercer"}
  path="[[docs.#{component.type.pluralize}.#{component.name}]]"
  status={"prod-ready"}
  type={"transform"} />
<Component
  delivery_guarantee={null}
  description={"Accepts `log` and `metric` events and allows you to filter events by a log field's value."}
  event_types={["log","metric"]}
  id={"field_filter_transform"}
  name={"field_filter"}
  path="[[docs.#{component.type.pluralize}.#{component.name}]]"
  status={"beta"}
  type={"transform"} />
<Component
  delivery_guarantee={null}
  description={"Accepts `log` events and allows you to parse a log field value with Grok."}
  event_types={["log"]}
  id={"grok_parser_transform"}
  name={"grok_parser"}
  path="[[docs.#{component.type.pluralize}.#{component.name}]]"
  status={"prod-ready"}
  type={"transform"} />
<Component
  delivery_guarantee={null}
  description={"Accepts `log` events and allows you to parse a log field value as JSON."}
  event_types={["log"]}
  id={"json_parser_transform"}
  name={"json_parser"}
  path="[[docs.#{component.type.pluralize}.#{component.name}]]"
  status={"prod-ready"}
  type={"transform"} />
<Component
  delivery_guarantee={null}
  description={"Accepts `log` events and allows you to convert logs into one or more metrics."}
  event_types={["log","metric"]}
  id={"log_to_metric_transform"}
  name={"log_to_metric"}
  path="[[docs.#{component.type.pluralize}.#{component.name}]]"
  status={"prod-ready"}
  type={"transform"} />
<Component
  delivery_guarantee={null}
  description={"Accepts `log` events and allows you to transform events with a full embedded Lua engine."}
  event_types={["log"]}
  id={"lua_transform"}
  name={"lua"}
  path="[[docs.#{component.type.pluralize}.#{component.name}]]"
  status={"beta"}
  type={"transform"} />
<Component
  delivery_guarantee={null}
  description={"Accepts `log` events and allows you to parse a log field's value with a Regular Expression."}
  event_types={["log"]}
  id={"regex_parser_transform"}
  name={"regex_parser"}
  path="[[docs.#{component.type.pluralize}.#{component.name}]]"
  status={"prod-ready"}
  type={"transform"} />
<Component
  delivery_guarantee={null}
  description={"Accepts `log` events and allows you to remove one or more log fields."}
  event_types={["log"]}
  id={"remove_fields_transform"}
  name={"remove_fields"}
  path="[[docs.#{component.type.pluralize}.#{component.name}]]"
  status={"prod-ready"}
  type={"transform"} />
<Component
  delivery_guarantee={null}
  description={"Accepts `metric` events and allows you to remove one or more metric tags."}
  event_types={["metric"]}
  id={"remove_tags_transform"}
  name={"remove_tags"}
  path="[[docs.#{component.type.pluralize}.#{component.name}]]"
  status={"prod-ready"}
  type={"transform"} />
<Component
  delivery_guarantee={null}
  description={"Accepts `log` events and allows you to sample events with a configurable rate."}
  event_types={["log"]}
  id={"sampler_transform"}
  name={"sampler"}
  path="[[docs.#{component.type.pluralize}.#{component.name}]]"
  status={"beta"}
  type={"transform"} />
<Component
  delivery_guarantee={null}
  description={"Accepts `log` events and allows you to split a field's value on a given separator and zip the tokens into ordered field names."}
  event_types={["log"]}
  id={"split_transform"}
  name={"split"}
  path="[[docs.#{component.type.pluralize}.#{component.name}]]"
  status={"prod-ready"}
  type={"transform"} />
<Component
  delivery_guarantee={null}
  description={"Accepts `log` events and allows you to tokenize a field's value by splitting on white space, ignoring special wrapping characters, and zip the tokens into ordered field names."}
  event_types={["log"]}
  id={"tokenizer_transform"}
  name={"tokenizer"}
  path="[[docs.#{component.type.pluralize}.#{component.name}]]"
  status={"prod-ready"}
  type={"transform"} />
<Component
  delivery_guarantee={"at_least_once"}
  description={"Batches `log` events to AWS CloudWatch Logs via the `PutLogEvents` API endpoint."}
  event_types={["log"]}
  id={"aws_cloudwatch_logs_sink"}
  name={"aws_cloudwatch_logs"}
  path="[[docs.#{component.type.pluralize}.#{component.name}]]"
  status={"beta"}
  type={"sink"} />
<Component
  delivery_guarantee={"at_least_once"}
  description={"Streams `metric` events to AWS CloudWatch Metrics via the `PutMetricData` API endpoint."}
  event_types={["metric"]}
  id={"aws_cloudwatch_metrics_sink"}
  name={"aws_cloudwatch_metrics"}
  path="[[docs.#{component.type.pluralize}.#{component.name}]]"
  status={"beta"}
  type={"sink"} />
<Component
  delivery_guarantee={"at_least_once"}
  description={"Batches `log` events to AWS Kinesis Data Stream via the `PutRecords` API endpoint."}
  event_types={["log"]}
  id={"aws_kinesis_streams_sink"}
  name={"aws_kinesis_streams"}
  path="[[docs.#{component.type.pluralize}.#{component.name}]]"
  status={"beta"}
  type={"sink"} />
<Component
  delivery_guarantee={"at_least_once"}
  description={"Batches `log` events to AWS S3 via the `PutObject` API endpoint."}
  event_types={["log"]}
  id={"aws_s3_sink"}
  name={"aws_s3"}
  path="[[docs.#{component.type.pluralize}.#{component.name}]]"
  status={"beta"}
  type={"sink"} />
<Component
  delivery_guarantee={"best_effort"}
  description={"Streams `log` and `metric` events to a blackhole that simply discards data, designed for testing and benchmarking purposes."}
  event_types={["log","metric"]}
  id={"blackhole_sink"}
  name={"blackhole"}
  path="[[docs.#{component.type.pluralize}.#{component.name}]]"
  status={"prod-ready"}
  type={"sink"} />
<Component
  delivery_guarantee={"best_effort"}
  description={"Batches `log` events to Clickhouse via the `HTTP` Interface."}
  event_types={["log"]}
  id={"clickhouse_sink"}
  name={"clickhouse"}
  path="[[docs.#{component.type.pluralize}.#{component.name}]]"
  status={"beta"}
  type={"sink"} />
<Component
  delivery_guarantee={"best_effort"}
  description={"Streams `log` and `metric` events to standard output streams, such as `STDOUT` and `STDERR`."}
  event_types={["log","metric"]}
  id={"console_sink"}
  name={"console"}
  path="[[docs.#{component.type.pluralize}.#{component.name}]]"
  status={"prod-ready"}
  type={"sink"} />
<Component
  delivery_guarantee={"best_effort"}
  description={"Batches `metric` events to Datadog metrics service using HTTP API."}
  event_types={["metric"]}
  id={"datadog_metrics_sink"}
  name={"datadog_metrics"}
  path="[[docs.#{component.type.pluralize}.#{component.name}]]"
  status={"beta"}
  type={"sink"} />
<Component
  delivery_guarantee={"best_effort"}
  description={"Batches `log` events to Elasticsearch via the `_bulk` API endpoint."}
  event_types={["log"]}
  id={"elasticsearch_sink"}
  name={"elasticsearch"}
  path="[[docs.#{component.type.pluralize}.#{component.name}]]"
  status={"beta"}
  type={"sink"} />
<Component
  delivery_guarantee={"best_effort"}
  description={"Streams `log` events to a file."}
  event_types={["log"]}
  id={"file_sink"}
  name={"file"}
  path="[[docs.#{component.type.pluralize}.#{component.name}]]"
  status={"prod-ready"}
  type={"sink"} />
<Component
  delivery_guarantee={"at_least_once"}
  description={"Batches `log` events to a generic HTTP endpoint."}
  event_types={["log"]}
  id={"http_sink"}
  name={"http"}
  path="[[docs.#{component.type.pluralize}.#{component.name}]]"
  status={"prod-ready"}
  type={"sink"} />
<Component
  delivery_guarantee={"at_least_once"}
  description={"Streams `log` events to Apache Kafka via the Kafka protocol."}
  event_types={["log"]}
  id={"kafka_sink"}
  name={"kafka"}
  path="[[docs.#{component.type.pluralize}.#{component.name}]]"
  status={"prod-ready"}
  type={"sink"} />
<Component
  delivery_guarantee={"best_effort"}
  description={"Exposes `metric` events to Prometheus metrics service."}
  event_types={["metric"]}
  id={"prometheus_sink"}
  name={"prometheus"}
  path="[[docs.#{component.type.pluralize}.#{component.name}]]"
  status={"beta"}
  type={"sink"} />
<Component
  delivery_guarantee={"at_least_once"}
  description={"Batches `log` events to a Splunk HTTP Event Collector."}
  event_types={["log"]}
  id={"splunk_hec_sink"}
  name={"splunk_hec"}
  path="[[docs.#{component.type.pluralize}.#{component.name}]]"
  status={"prod-ready"}
  type={"sink"} />
<Component
  delivery_guarantee={"best_effort"}
  description={"Streams `metric` events to StatsD metrics service."}
  event_types={["metric"]}
  id={"statsd_sink"}
  name={"statsd"}
  path="[[docs.#{component.type.pluralize}.#{component.name}]]"
  status={"beta"}
  type={"sink"} />
<Component
  delivery_guarantee={"best_effort"}
  description={"Streams `log` events to a TCP connection."}
  event_types={["log"]}
  id={"tcp_sink"}
  name={"tcp"}
  path="[[docs.#{component.type.pluralize}.#{component.name}]]"
  status={"prod-ready"}
  type={"sink"} />
<Component
  delivery_guarantee={"best_effort"}
  description={"Streams `log` events to another downstream Vector instance."}
  event_types={["log"]}
  id={"vector_sink"}
  name={"vector"}
  path="[[docs.#{component.type.pluralize}.#{component.name}]]"
  status={"prod-ready"}
  type={"sink"} />

</Components>

import Jump from '@site/src/components/Jump';

<Jump to="https://github.com/timberio/vector/issues/new?labels=Type%3A+New+Feature" icon="plus-circle">
  Request a new component
</Jump>


[docs.configuration#composition]: ./setup/configuration#composition
