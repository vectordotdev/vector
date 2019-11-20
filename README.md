<p align="center">
  <strong>
    <a href="https://vector.dev/community">Chat/Forum<a/>&nbsp;&nbsp;&bull;&nbsp;&nbsp;<a href="https://vector.dev/mailing_list/">Mailing List<a/>&nbsp;&nbsp;&bull;&nbsp;&nbsp;<a href="https://docs.vector.dev/setup/installation">Install 0.5.0<a/>
  </strong>
</p>

---

<p align="center">
  <img src="./website/static/img/readme_diagram.svg" alt="Vector">
</p>

Vector is a [high-performance][docs.performance] observability data router. It
makes [collecting][docs.sources], [transforming][docs.transforms], and
[sending][docs.sinks] logs, metrics, and events easy. It decouples data
collection & routing from your services, giving you control and data ownership,
among [many other benefits][docs.use_cases].

Built in [Rust][urls.rust], Vector places high-value on
[performance][docs.performance], [correctness][docs.correctness], and [operator
friendliness][docs.administration]. It compiles to a single static binary and is
designed to be [deployed][docs.deployment] across your entire infrastructure,
serving both as a light-weight [agent][docs.roles.agent] and a highly efficient
[service][docs.roles.service], making the process of getting data from A to B
simple and unified.


## [Documentation](https://docs.vector.dev/)

#### About

* [**Concepts**][docs.concepts]
* [**Data model**][docs.data_model] - [log event][docs.data-model.log], [metric event][docs.data-model.metric]
* [**Guarantees**][docs.guarantees]

#### Setup

* [**Installation**][docs.installation] - [containers][docs.containers], [operating systems][docs.operating_systems], [package managers][docs.package_managers], [from archives][docs.from-archives], [from source][docs.from-source]
* [**Configuration**][docs.configuration]
* [**Deployment**][docs.deployment] - [topologies][docs.topologies], [roles][docs.roles]

#### [Components][https://vector.dev/components]

* [**Sources**][docs.sources] - 
* [**Transforms**][docs.transforms]
* [**Sinks**][docs.sinks]

* [**Administration**][docs.administration] - [process management][docs.process-management], [monitoring][docs.monitoring], [updating][docs.updating], [validating][docs.validating]
* [**Guides**][docs.guides]

#### Resources

* [**Community**][urls.vector_community] - [chat/forum][urls.vector_chat], [mailing list][urls.mailing_list]
* [**Releases**][urls.vector_releases] - [v0.5.0][urls.v0.5.0], [changelog][urls.vector_changelog]
* [**Roadmap**][urls.vector_roadmap] - [vote on new features][urls.vote_feature]


## Features

* ***Fast*** - Built in [Rust][urls.rust], Vector is [fast and memory efficient][docs.performance]. No runtime. No garbage collector.
* **Correct** - Obsessed with [getting the details right][docs.correctness].
* **Vendor Neutral** - Does not favor a specific storage. Fair, open, with the user's best interest in mind.
* **Agent or Service** - One simple tool to get data from A to B. Deploys as an [agent][docs.roles.agent] or [service][docs.roles.service].
* **Logs, Metrics, or Events** - [Logs][docs.data-model.log], [metrics][docs.data-model.metric], and [events][docs.data_model]. Collect, unify, and ship all observability data.
* **Correlate Logs & Metrics** - [Derive metrics from logs][docs.transforms.log_to_metric], add shared context with [transforms][docs.transforms].
* **Clear Guarantees** - A [guarantee support matrix][docs.guarantees] helps you understand your tradeoffs.
* **Easy To Deploy** - Cross-compiles to [a single static binary][docs.archives] with no runtime.
* **Hot Reload** - [Reload configuration on the fly][docs.process-management#reloading], without skipping a beat.


## Performance

| Test | Vector | Filebeat | FluentBit | FluentD | Logstash | SplunkUF | SplunkHF |
| ---: | :---: | :---: | :---: | :---: | :---: | :---: | :---: |
| [TCP to Blackhole](https://github.com/timberio/vector-test-harness/tree/master/cases/tcp_to_blackhole_performance) | _**86mib/s**_ | n/a | 64.4mib/s | 27.7mib/s | 40.6mib/s | n/a | n/a |
| [File to TCP](https://github.com/timberio/vector-test-harness/tree/master/cases/file_to_tcp_performance) | _**76.7mib/s**_ | 7.8mib/s | 35mib/s | 26.1mib/s | 3.1mib/s | 40.1mib/s | 39mib/s |
| [Regex Parsing](https://github.com/timberio/vector-test-harness/tree/master/cases/regex_parsing_performance) | 13.2mib/s | n/a | _**20.5mib/s**_ | 2.6mib/s | 4.6mib/s | n/a | 7.8mib/s |
| [TCP to HTTP](https://github.com/timberio/vector-test-harness/tree/master/cases/tcp_to_http_performance) | _**26.7mib/s**_ | n/a | 19.6mib/s | <1mib/s | 2.7mib/s | n/a | n/a |
| [TCP to TCP](https://github.com/timberio/vector-test-harness/tree/master/cases/tcp_to_tcp_performance) | 69.9mib/s | 5mib/s | 67.1mib/s | 3.9mib/s | 10mib/s | _**70.4mib/s**_ | 7.6mib/s |

To learn more about our performance tests, please see the [Vector test harness][urls.test_harness].


## Correctness

| Test | Vector | Filebeat | FluentBit | FluentD | Logstash | Splunk UF | Splunk HF |
| ---: | :---: | :---: | :---: | :---: | :---: | :---: | :---: |
| [Disk Buffer Persistence](https://github.com/timberio/vector-test-harness/tree/master/cases/disk_buffer_persistence_correctness) | ✅ | ✅ | ❌ | ❌ | ⚠️ | ✅ | ✅ |
| [File Rotate (create)](https://github.com/timberio/vector-test-harness/tree/master/cases/file_rotate_create_correctness) | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| [File Rotate (copytruncate)](https://github.com/timberio/vector-test-harness/tree/master/cases/file_rotate_truncate_correctness) | ✅ | ❌ | ❌ | ❌ | ❌ | ✅ | ✅ |
| [File Truncation](https://github.com/timberio/vector-test-harness/tree/master/cases/file_truncate_correctness) | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| [Process (SIGHUP)](https://github.com/timberio/vector-test-harness/tree/master/cases/sighup_correctness) | ✅ | ❌ | ❌ | ❌ | ⚠️ | ✅ | ✅ |
| [JSON (wrapped)](https://github.com/timberio/vector-test-harness/tree/master/cases/wrapped_json_correctness) | ✅ | ✅ | ❌ | ✅ | ✅ | ✅ | ✅ |

To learn more about our performance tests, please see the [Vector test harness][urls.test_harness].


## Installation

Run the following in your terminal, then follow the on-screen instructions.

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.vector.dev | sh
```

Or view [platform specific installation instructions][docs.installation].


## Sources

| Name  | Description |
|:------|:------------|
| [**`docker`**][docs.sources.docker] | Ingests data through the docker engine daemon and outputs [`log`][docs.data-model#log] events. |
| [**`file`**][docs.sources.file] | Ingests data through one or more local files and outputs [`log`][docs.data-model#log] events. |
| [**`journald`**][docs.sources.journald] | Ingests data through log records from journald and outputs [`log`][docs.data-model#log] events. |
| [**`kafka`**][docs.sources.kafka] | Ingests data through Kafka 0.9 or later and outputs [`log`][docs.data-model#log] events. |
| [**`statsd`**][docs.sources.statsd] | Ingests data through the StatsD UDP protocol and outputs [`metric`][docs.data-model#metric] events. |
| [**`stdin`**][docs.sources.stdin] | Ingests data through standard input (STDIN) and outputs [`log`][docs.data-model#log] events. |
| [**`syslog`**][docs.sources.syslog] | Ingests data through the Syslog 5424 protocol and outputs [`log`][docs.data-model#log] events. |
| [**`tcp`**][docs.sources.tcp] | Ingests data through the TCP protocol and outputs [`log`][docs.data-model#log] events. |
| [**`udp`**][docs.sources.udp] | Ingests data through the UDP protocol and outputs [`log`][docs.data-model#log] events. |
| [**`vector`**][docs.sources.vector] | Ingests data through another upstream [`vector` sink][docs.sinks.vector] and outputs [`log`][docs.data-model#log] and [`metric`][docs.data-model#metric] events. |

[+ request a new source][urls.new_source]


## Transforms

| Name  | Description |
|:------|:------------|
| [**`add_fields`**][docs.transforms.add_fields] | Accepts [`log`][docs.data-model#log] events and allows you to add one or more log fields. |
| [**`add_tags`**][docs.transforms.add_tags] | Accepts [`metric`][docs.data-model#metric] events and allows you to add one or more metric tags. |
| [**`coercer`**][docs.transforms.coercer] | Accepts [`log`][docs.data-model#log] events and allows you to coerce log fields into fixed types. |
| [**`field_filter`**][docs.transforms.field_filter] | Accepts [`log`][docs.data-model#log] and [`metric`][docs.data-model#metric] events and allows you to filter events by a log field's value. |
| [**`grok_parser`**][docs.transforms.grok_parser] | Accepts [`log`][docs.data-model#log] events and allows you to parse a log field value with [Grok][urls.grok]. |
| [**`json_parser`**][docs.transforms.json_parser] | Accepts [`log`][docs.data-model#log] events and allows you to parse a log field value as JSON. |
| [**`log_to_metric`**][docs.transforms.log_to_metric] | Accepts [`log`][docs.data-model#log] events and allows you to convert logs into one or more metrics. |
| [**`lua`**][docs.transforms.lua] | Accepts [`log`][docs.data-model#log] events and allows you to transform events with a full embedded [Lua][urls.lua] engine. |
| [**`regex_parser`**][docs.transforms.regex_parser] | Accepts [`log`][docs.data-model#log] events and allows you to parse a log field's value with a [Regular Expression][urls.regex]. |
| [**`remove_fields`**][docs.transforms.remove_fields] | Accepts [`log`][docs.data-model#log] events and allows you to remove one or more log fields. |
| [**`remove_tags`**][docs.transforms.remove_tags] | Accepts [`metric`][docs.data-model#metric] events and allows you to remove one or more metric tags. |
| [**`sampler`**][docs.transforms.sampler] | Accepts [`log`][docs.data-model#log] events and allows you to sample events with a configurable rate. |
| [**`split`**][docs.transforms.split] | Accepts [`log`][docs.data-model#log] events and allows you to split a field's value on a given separator and zip the tokens into ordered field names. |
| [**`tokenizer`**][docs.transforms.tokenizer] | Accepts [`log`][docs.data-model#log] events and allows you to tokenize a field's value by splitting on white space, ignoring special wrapping characters, and zip the tokens into ordered field names. |

[+ request a new transform][urls.new_transform]


## Sinks

| Name  | Description |
|:------|:------------|
| [**`aws_cloudwatch_logs`**][docs.sinks.aws_cloudwatch_logs] | [Batches](#buffers-and-batches) [`log`][docs.data-model#log] events to [AWS CloudWatch Logs][urls.aws_cw_logs] via the [`PutLogEvents` API endpoint](https://docs.aws.amazon.com/AmazonCloudWatchLogs/latest/APIReference/API_PutLogEvents.html). |
| [**`aws_cloudwatch_metrics`**][docs.sinks.aws_cloudwatch_metrics] | [Streams](#streaming) [`metric`][docs.data-model#metric] events to [AWS CloudWatch Metrics][urls.aws_cw_metrics] via the [`PutMetricData` API endpoint](https://docs.aws.amazon.com/AmazonCloudWatch/latest/APIReference/API_PutMetricData.html). |
| [**`aws_kinesis_streams`**][docs.sinks.aws_kinesis_streams] | [Batches](#buffers-and-batches) [`log`][docs.data-model#log] events to [AWS Kinesis Data Stream][urls.aws_kinesis_data_streams] via the [`PutRecords` API endpoint](https://docs.aws.amazon.com/kinesis/latest/APIReference/API_PutRecords.html). |
| [**`aws_s3`**][docs.sinks.aws_s3] | [Batches](#buffers-and-batches) [`log`][docs.data-model#log] events to [AWS S3][urls.aws_s3] via the [`PutObject` API endpoint](https://docs.aws.amazon.com/AmazonS3/latest/API/RESTObjectPUT.html). |
| [**`blackhole`**][docs.sinks.blackhole] | [Streams](#streaming) [`log`][docs.data-model#log] and [`metric`][docs.data-model#metric] events to a blackhole that simply discards data, designed for testing and benchmarking purposes. |
| [**`clickhouse`**][docs.sinks.clickhouse] | [Batches](#buffers-and-batches) [`log`][docs.data-model#log] events to [Clickhouse][urls.clickhouse] via the [`HTTP` Interface][urls.clickhouse_http]. |
| [**`console`**][docs.sinks.console] | [Streams](#streaming) [`log`][docs.data-model#log] and [`metric`][docs.data-model#metric] events to [standard output streams][urls.standard_streams], such as `STDOUT` and `STDERR`. |
| [**`datadog_metrics`**][docs.sinks.datadog_metrics] | [Batches](#buffers-and-batches) [`metric`][docs.data-model#metric] events to [Datadog][urls.datadog] metrics service using [HTTP API](https://docs.datadoghq.com/api/?lang=bash#metrics). |
| [**`elasticsearch`**][docs.sinks.elasticsearch] | [Batches](#buffers-and-batches) [`log`][docs.data-model#log] events to [Elasticsearch][urls.elasticsearch] via the [`_bulk` API endpoint](https://www.elastic.co/guide/en/elasticsearch/reference/current/docs-bulk.html). |
| [**`file`**][docs.sinks.file] | [Streams](#streaming) [`log`][docs.data-model#log] events to a file. |
| [**`http`**][docs.sinks.http] | [Batches](#buffers-and-batches) [`log`][docs.data-model#log] events to a generic HTTP endpoint. |
| [**`kafka`**][docs.sinks.kafka] | [Streams](#streaming) [`log`][docs.data-model#log] events to [Apache Kafka][urls.kafka] via the [Kafka protocol][urls.kafka_protocol]. |
| [**`prometheus`**][docs.sinks.prometheus] | [Exposes](#exposing-and-scraping) [`metric`][docs.data-model#metric] events to [Prometheus][urls.prometheus] metrics service. |
| [**`splunk_hec`**][docs.sinks.splunk_hec] | [Batches](#buffers-and-batches) [`log`][docs.data-model#log] events to a [Splunk HTTP Event Collector][urls.splunk_hec]. |
| [**`statsd`**][docs.sinks.statsd] | [Streams](#streaming) [`metric`][docs.data-model#metric] events to [StatsD][urls.statsd] metrics service. |
| [**`tcp`**][docs.sinks.tcp] | [Streams](#streaming) [`log`][docs.data-model#log] events to a TCP connection. |
| [**`vector`**][docs.sinks.vector] | [Streams](#streaming) [`log`][docs.data-model#log] events to another downstream [`vector` source][docs.sources.vector]. |

[+ request a new sink][urls.new_sink]


## License

Copyright 2019, Vector Authors. All rights reserved.

Licensed under the Apache License, Version 2.0 (the "License"); you may not
use these files except in compliance with the License. You may obtain a copy
of the License at

http://www.apache.org/licenses/LICENSE-2.0

Unless required by applicable law or agreed to in writing, software
distributed under the License is distributed on an "AS IS" BASIS, WITHOUT
WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied. See the
License for the specific language governing permissions and limitations under
the License.

---

<p align="center">
  Developed with ❤️ by <strong><a href="https://timber.io">Timber.io</a></strong>
</p>


[docs.administration]: https://vector.dev/docs/administration
[docs.archives]: https://vector.dev/docs/setup/installation/manual/from-archives
[docs.concepts]: https://vector.dev/docs/about/concepts
[docs.configuration]: https://vector.dev/docs/setup/configuration
[docs.containers]: https://vector.dev/docs/setup/installation/containers
[docs.correctness]: https://vector.dev/docs/about/correctness
[docs.data-model#log]: https://vector.dev/docs/about/data-model#log
[docs.data-model#metric]: https://vector.dev/docs/about/data-model#metric
[docs.data-model.log]: https://vector.dev/docs/about/data-model/log
[docs.data-model.metric]: https://vector.dev/docs/about/data-model/metric
[docs.data_model]: https://vector.dev/docs/about/data-model
[docs.deployment]: https://vector.dev/docs/setup/deployment
[docs.from-archives]: https://vector.dev/docs/setup/installation/manual/from-archives
[docs.from-source]: https://vector.dev/docs/setup/installation/manual/from-source
[docs.guarantees]: https://vector.dev/docs/about/guarantees
[docs.guides]: https://vector.dev/docs/setup/guides
[docs.installation]: https://vector.dev/docs/setup/installation
[docs.monitoring]: https://vector.dev/docs/administration/monitoring
[docs.operating_systems]: https://vector.dev/docs/setup/installation/operating-systems
[docs.package_managers]: https://vector.dev/docs/setup/installation/package-managers
[docs.performance]: https://vector.dev/docs/about/performance
[docs.process-management#reloading]: https://vector.dev/docs/administration/process-management#reloading
[docs.process-management]: https://vector.dev/docs/administration/process-management
[docs.roles.agent]: https://vector.dev/docs/setup/deployment/roles/agent
[docs.roles.service]: https://vector.dev/docs/setup/deployment/roles/service
[docs.roles]: https://vector.dev/docs/setup/deployment/roles
[docs.sinks.aws_cloudwatch_logs]: https://vector.dev/docs/components/sinks/aws_cloudwatch_logs
[docs.sinks.aws_cloudwatch_metrics]: https://vector.dev/docs/components/sinks/aws_cloudwatch_metrics
[docs.sinks.aws_kinesis_streams]: https://vector.dev/docs/components/sinks/aws_kinesis_streams
[docs.sinks.aws_s3]: https://vector.dev/docs/components/sinks/aws_s3
[docs.sinks.blackhole]: https://vector.dev/docs/components/sinks/blackhole
[docs.sinks.clickhouse]: https://vector.dev/docs/components/sinks/clickhouse
[docs.sinks.console]: https://vector.dev/docs/components/sinks/console
[docs.sinks.datadog_metrics]: https://vector.dev/docs/components/sinks/datadog_metrics
[docs.sinks.elasticsearch]: https://vector.dev/docs/components/sinks/elasticsearch
[docs.sinks.file]: https://vector.dev/docs/components/sinks/file
[docs.sinks.http]: https://vector.dev/docs/components/sinks/http
[docs.sinks.kafka]: https://vector.dev/docs/components/sinks/kafka
[docs.sinks.prometheus]: https://vector.dev/docs/components/sinks/prometheus
[docs.sinks.splunk_hec]: https://vector.dev/docs/components/sinks/splunk_hec
[docs.sinks.statsd]: https://vector.dev/docs/components/sinks/statsd
[docs.sinks.tcp]: https://vector.dev/docs/components/sinks/tcp
[docs.sinks.vector]: https://vector.dev/docs/components/sinks/vector
[docs.sinks]: https://vector.dev/docs/components/sinks
[docs.sources.docker]: https://vector.dev/docs/components/sources/docker
[docs.sources.file]: https://vector.dev/docs/components/sources/file
[docs.sources.journald]: https://vector.dev/docs/components/sources/journald
[docs.sources.kafka]: https://vector.dev/docs/components/sources/kafka
[docs.sources.statsd]: https://vector.dev/docs/components/sources/statsd
[docs.sources.stdin]: https://vector.dev/docs/components/sources/stdin
[docs.sources.syslog]: https://vector.dev/docs/components/sources/syslog
[docs.sources.tcp]: https://vector.dev/docs/components/sources/tcp
[docs.sources.udp]: https://vector.dev/docs/components/sources/udp
[docs.sources.vector]: https://vector.dev/docs/components/sources/vector
[docs.sources]: https://vector.dev/docs/components/sources
[docs.topologies]: https://vector.dev/docs/setup/deployment/topologies
[docs.transforms.add_fields]: https://vector.dev/docs/components/transforms/add_fields
[docs.transforms.add_tags]: https://vector.dev/docs/components/transforms/add_tags
[docs.transforms.coercer]: https://vector.dev/docs/components/transforms/coercer
[docs.transforms.field_filter]: https://vector.dev/docs/components/transforms/field_filter
[docs.transforms.grok_parser]: https://vector.dev/docs/components/transforms/grok_parser
[docs.transforms.json_parser]: https://vector.dev/docs/components/transforms/json_parser
[docs.transforms.log_to_metric]: https://vector.dev/docs/components/transforms/log_to_metric
[docs.transforms.lua]: https://vector.dev/docs/components/transforms/lua
[docs.transforms.regex_parser]: https://vector.dev/docs/components/transforms/regex_parser
[docs.transforms.remove_fields]: https://vector.dev/docs/components/transforms/remove_fields
[docs.transforms.remove_tags]: https://vector.dev/docs/components/transforms/remove_tags
[docs.transforms.sampler]: https://vector.dev/docs/components/transforms/sampler
[docs.transforms.split]: https://vector.dev/docs/components/transforms/split
[docs.transforms.tokenizer]: https://vector.dev/docs/components/transforms/tokenizer
[docs.transforms]: https://vector.dev/docs/components/transforms
[docs.updating]: https://vector.dev/docs/administration/updating
[docs.use_cases]: https://vector.dev/docs/use_cases
[docs.validating]: https://vector.dev/docs/administration/validating
[urls.aws_cw_logs]: https://docs.aws.amazon.com/AmazonCloudWatch/latest/logs/WhatIsCloudWatchLogs.html
[urls.aws_cw_metrics]: https://docs.aws.amazon.com/AmazonCloudWatch/latest/monitoring/working_with_metrics.html
[urls.aws_kinesis_data_streams]: https://aws.amazon.com/kinesis/data-streams/
[urls.aws_s3]: https://aws.amazon.com/s3/
[urls.clickhouse]: https://clickhouse.yandex/
[urls.clickhouse_http]: https://clickhouse.yandex/docs/en/interfaces/http/
[urls.datadog]: https://www.datadoghq.com
[urls.elasticsearch]: https://www.elastic.co/products/elasticsearch
[urls.grok]: http://grokdebug.herokuapp.com/
[urls.kafka]: https://kafka.apache.org/
[urls.kafka_protocol]: https://kafka.apache.org/protocol
[urls.lua]: https://www.lua.org/
[urls.mailing_list]: https://vector.dev/mailing_list/
[urls.new_sink]: https://github.com/timberio/vector/issues/new?labels=Type%3A+New+Feature
[urls.new_source]: https://github.com/timberio/vector/issues/new?labels=Type%3A+New+Feature
[urls.new_transform]: https://github.com/timberio/vector/issues/new?labels=Type%3A+New+Feature
[urls.prometheus]: https://prometheus.io/
[urls.regex]: https://en.wikipedia.org/wiki/Regular_expression
[urls.rust]: https://www.rust-lang.org/
[urls.splunk_hec]: http://dev.splunk.com/view/event-collector/SP-CAAAE6M
[urls.standard_streams]: https://en.wikipedia.org/wiki/Standard_streams
[urls.statsd]: https://github.com/statsd/statsd
[urls.test_harness]: https://github.com/timberio/vector-test-harness/
[urls.v0.5.0]: https://github.com/timberio/vector/releases/tag/v0.5.0
[urls.vector_changelog]: https://github.com/timberio/vector/blob/master/CHANGELOG.md
[urls.vector_chat]: https://chat.vector.dev
[urls.vector_community]: https://vector.dev/community
[urls.vector_releases]: https://github.com/timberio/vector/releases
[urls.vector_roadmap]: https://github.com/timberio/vector/milestones?direction=asc&sort=due_date&state=open
[urls.vote_feature]: https://github.com/timberio/vector/issues?q=is%3Aissue+is%3Aopen+sort%3Areactions-%2B1-desc+label%3A%22Type%3A+New+Feature%22
