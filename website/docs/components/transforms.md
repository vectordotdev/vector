---
title: Transforms
sidebar_label: hidden
---

Transforms are in the middle of the [pipeline][docs.configuration#composition],
sitting in-between [sources][docs.sources] and [sinks][docs.sinks]. They
transform [events][docs.data-model#event] or the stream as a whole.

import Components from '@site/src/components/Components';

import Component from '@site/src/components/Component';

<Components>

<Component
  delivery_guarantee={"best_effort"}
  event_types={["log"]}
  id={"docker_source"}
  name={"docker"}
  path={"/components/sources/docker"}
  status={"beta"}
  type={"source"} />
<Component
  delivery_guarantee={"best_effort"}
  event_types={["log"]}
  id={"file_source"}
  name={"file"}
  path={"/components/sources/file"}
  status={"prod-ready"}
  type={"source"} />
<Component
  delivery_guarantee={"best_effort"}
  event_types={["log"]}
  id={"journald_source"}
  name={"journald"}
  path={"/components/sources/journald"}
  status={"beta"}
  type={"source"} />
<Component
  delivery_guarantee={"at_least_once"}
  event_types={["log"]}
  id={"kafka_source"}
  name={"kafka"}
  path={"/components/sources/kafka"}
  status={"beta"}
  type={"source"} />
<Component
  delivery_guarantee={"best_effort"}
  event_types={["metric"]}
  id={"statsd_source"}
  name={"statsd"}
  path={"/components/sources/statsd"}
  status={"beta"}
  type={"source"} />
<Component
  delivery_guarantee={"at_least_once"}
  event_types={["log"]}
  id={"stdin_source"}
  name={"stdin"}
  path={"/components/sources/stdin"}
  status={"prod-ready"}
  type={"source"} />
<Component
  delivery_guarantee={"best_effort"}
  event_types={["log"]}
  id={"syslog_source"}
  name={"syslog"}
  path={"/components/sources/syslog"}
  status={"prod-ready"}
  type={"source"} />
<Component
  delivery_guarantee={"best_effort"}
  event_types={["log"]}
  id={"tcp_source"}
  name={"tcp"}
  path={"/components/sources/tcp"}
  status={"prod-ready"}
  type={"source"} />
<Component
  delivery_guarantee={"best_effort"}
  event_types={["log"]}
  id={"udp_source"}
  name={"udp"}
  path={"/components/sources/udp"}
  status={"prod-ready"}
  type={"source"} />
<Component
  delivery_guarantee={"best_effort"}
  event_types={["log","metric"]}
  id={"vector_source"}
  name={"vector"}
  path={"/components/sources/vector"}
  status={"beta"}
  type={"source"} />
<Component
  delivery_guarantee={null}
  event_types={["log"]}
  id={"add_fields_transform"}
  name={"add_fields"}
  path={"/components/transforms/add_fields"}
  status={"prod-ready"}
  type={"transform"} />
<Component
  delivery_guarantee={null}
  event_types={["metric"]}
  id={"add_tags_transform"}
  name={"add_tags"}
  path={"/components/transforms/add_tags"}
  status={"prod-ready"}
  type={"transform"} />
<Component
  delivery_guarantee={null}
  event_types={["log"]}
  id={"coercer_transform"}
  name={"coercer"}
  path={"/components/transforms/coercer"}
  status={"prod-ready"}
  type={"transform"} />
<Component
  delivery_guarantee={null}
  event_types={["log","metric"]}
  id={"field_filter_transform"}
  name={"field_filter"}
  path={"/components/transforms/field_filter"}
  status={"beta"}
  type={"transform"} />
<Component
  delivery_guarantee={null}
  event_types={["log"]}
  id={"grok_parser_transform"}
  name={"grok_parser"}
  path={"/components/transforms/grok_parser"}
  status={"prod-ready"}
  type={"transform"} />
<Component
  delivery_guarantee={null}
  event_types={["log"]}
  id={"json_parser_transform"}
  name={"json_parser"}
  path={"/components/transforms/json_parser"}
  status={"prod-ready"}
  type={"transform"} />
<Component
  delivery_guarantee={null}
  event_types={["log","metric"]}
  id={"log_to_metric_transform"}
  name={"log_to_metric"}
  path={"/components/transforms/log_to_metric"}
  status={"prod-ready"}
  type={"transform"} />
<Component
  delivery_guarantee={null}
  event_types={["log"]}
  id={"lua_transform"}
  name={"lua"}
  path={"/components/transforms/lua"}
  status={"beta"}
  type={"transform"} />
<Component
  delivery_guarantee={null}
  event_types={["log"]}
  id={"regex_parser_transform"}
  name={"regex_parser"}
  path={"/components/transforms/regex_parser"}
  status={"prod-ready"}
  type={"transform"} />
<Component
  delivery_guarantee={null}
  event_types={["log"]}
  id={"remove_fields_transform"}
  name={"remove_fields"}
  path={"/components/transforms/remove_fields"}
  status={"prod-ready"}
  type={"transform"} />
<Component
  delivery_guarantee={null}
  event_types={["metric"]}
  id={"remove_tags_transform"}
  name={"remove_tags"}
  path={"/components/transforms/remove_tags"}
  status={"prod-ready"}
  type={"transform"} />
<Component
  delivery_guarantee={null}
  event_types={["log"]}
  id={"sampler_transform"}
  name={"sampler"}
  path={"/components/transforms/sampler"}
  status={"beta"}
  type={"transform"} />
<Component
  delivery_guarantee={null}
  event_types={["log"]}
  id={"split_transform"}
  name={"split"}
  path={"/components/transforms/split"}
  status={"prod-ready"}
  type={"transform"} />
<Component
  delivery_guarantee={null}
  event_types={["log"]}
  id={"tokenizer_transform"}
  name={"tokenizer"}
  path={"/components/transforms/tokenizer"}
  status={"prod-ready"}
  type={"transform"} />
<Component
  delivery_guarantee={"at_least_once"}
  event_types={["log"]}
  id={"aws_cloudwatch_logs_sink"}
  name={"aws_cloudwatch_logs"}
  path={"/components/sinks/aws_cloudwatch_logs"}
  status={"beta"}
  type={"sink"} />
<Component
  delivery_guarantee={"at_least_once"}
  event_types={["metric"]}
  id={"aws_cloudwatch_metrics_sink"}
  name={"aws_cloudwatch_metrics"}
  path={"/components/sinks/aws_cloudwatch_metrics"}
  status={"beta"}
  type={"sink"} />
<Component
  delivery_guarantee={"at_least_once"}
  event_types={["log"]}
  id={"aws_kinesis_streams_sink"}
  name={"aws_kinesis_streams"}
  path={"/components/sinks/aws_kinesis_streams"}
  status={"beta"}
  type={"sink"} />
<Component
  delivery_guarantee={"at_least_once"}
  event_types={["log"]}
  id={"aws_s3_sink"}
  name={"aws_s3"}
  path={"/components/sinks/aws_s3"}
  status={"beta"}
  type={"sink"} />
<Component
  delivery_guarantee={"best_effort"}
  event_types={["log","metric"]}
  id={"blackhole_sink"}
  name={"blackhole"}
  path={"/components/sinks/blackhole"}
  status={"prod-ready"}
  type={"sink"} />
<Component
  delivery_guarantee={"best_effort"}
  event_types={["log"]}
  id={"clickhouse_sink"}
  name={"clickhouse"}
  path={"/components/sinks/clickhouse"}
  status={"beta"}
  type={"sink"} />
<Component
  delivery_guarantee={"best_effort"}
  event_types={["log","metric"]}
  id={"console_sink"}
  name={"console"}
  path={"/components/sinks/console"}
  status={"prod-ready"}
  type={"sink"} />
<Component
  delivery_guarantee={"best_effort"}
  event_types={["metric"]}
  id={"datadog_metrics_sink"}
  name={"datadog_metrics"}
  path={"/components/sinks/datadog_metrics"}
  status={"beta"}
  type={"sink"} />
<Component
  delivery_guarantee={"best_effort"}
  event_types={["log"]}
  id={"elasticsearch_sink"}
  name={"elasticsearch"}
  path={"/components/sinks/elasticsearch"}
  status={"beta"}
  type={"sink"} />
<Component
  delivery_guarantee={"best_effort"}
  event_types={["log"]}
  id={"file_sink"}
  name={"file"}
  path={"/components/sinks/file"}
  status={"prod-ready"}
  type={"sink"} />
<Component
  delivery_guarantee={"at_least_once"}
  event_types={["log"]}
  id={"http_sink"}
  name={"http"}
  path={"/components/sinks/http"}
  status={"prod-ready"}
  type={"sink"} />
<Component
  delivery_guarantee={"at_least_once"}
  event_types={["log"]}
  id={"kafka_sink"}
  name={"kafka"}
  path={"/components/sinks/kafka"}
  status={"prod-ready"}
  type={"sink"} />
<Component
  delivery_guarantee={"best_effort"}
  event_types={["metric"]}
  id={"prometheus_sink"}
  name={"prometheus"}
  path={"/components/sinks/prometheus"}
  status={"beta"}
  type={"sink"} />
<Component
  delivery_guarantee={"at_least_once"}
  event_types={["log"]}
  id={"splunk_hec_sink"}
  name={"splunk_hec"}
  path={"/components/sinks/splunk_hec"}
  status={"prod-ready"}
  type={"sink"} />
<Component
  delivery_guarantee={"best_effort"}
  event_types={["metric"]}
  id={"statsd_sink"}
  name={"statsd"}
  path={"/components/sinks/statsd"}
  status={"beta"}
  type={"sink"} />
<Component
  delivery_guarantee={"best_effort"}
  event_types={["log"]}
  id={"tcp_sink"}
  name={"tcp"}
  path={"/components/sinks/tcp"}
  status={"prod-ready"}
  type={"sink"} />
<Component
  delivery_guarantee={"best_effort"}
  event_types={["log"]}
  id={"vector_sink"}
  name={"vector"}
  path={"/components/sinks/vector"}
  status={"prod-ready"}
  type={"sink"} />

</Components>

[+ request a new transform][urls.new_transform]


[docs.configuration#composition]: ../setup/configuration#composition
[docs.data-model#event]: ../about/data-model#event
[docs.sinks]: ../components/sinks
[docs.sources]: ../components/sources
[urls.new_transform]: https://github.com/timberio/vector/issues/new?labels=Type%3A+New+Feature
