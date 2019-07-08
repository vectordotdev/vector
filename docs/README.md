<p align="center">
  <strong>
    <a href="https://chat.vector.dev">Slack Chat<a/>&nbsp;&nbsp;&bull;&nbsp;&nbsp;
    <a href="https://forum.vector.dev">Forums<a/>&nbsp;&nbsp;&bull;&nbsp;&nbsp;
    <a href="https://vector.dev/mailing_list">Mailing List<a/></strong>
</p>

---

<p align="center">
  <img src="./docs/assets/readme_diagram.svg" alt="Vector">
</p>

Vector is a [high-performance][docs.performance] observability data router. It
makes [collecting][docs.sources], [transforming][docs.transforms], and
[sending][docs.sinks] logs, metrics, and events easy. It decouples data
collection & routing from your services, giving you control and data ownership,
among [many other benefits][docs.use_cases].

Built in [Rust][url.rust], Vector places high-value on
[performance][docs.performance], [correctness][docs.correctness], and [operator
friendliness][docs.administration]. It compiles to a single static binary and is
designed to be [deployed][docs.deployment] across your entire infrastructure,
serving both as a light-weight [agent][docs.agent_role] and a highly efficient
[service][docs.service_role], making the process of getting data from A to B
simple and unified.


## [Documentation](https://docs.vector.dev/)

#### About

* [**Use cases**][docs.use_cases]
* [**Concepts**][docs.concepts]
* [**Data model**][docs.data_model]
* [**Guarantees**][docs.guarantees]

#### Setup

* [**Installation**][docs.installation] - [docker][docs.docker], [apt][docs.apt], [homebrew][docs.homebrew], [yum][docs.yum], and [more][docs.installation]
* [**Getting started**][docs.getting_started]
* [**Deployment**][docs.deployment] - [topologies][docs.topologies], [roles][docs.roles]

#### Usage

* [**Configuration**][docs.configuration] - [sources][docs.sources], [transforms][docs.transforms], [sinks][docs.sinks]
* [**Administration**][docs.administration] - [starting][docs.starting], [stopping][docs.stopping], [reloading][docs.reloading], [updating][docs.updating]
* [**Guides**][docs.guides]

#### Resources

* [**Community**][url.community] - [forum][url.vector_forum], [slack chat][url.vector_chat], [mailing list][url.mailing_list]
* [**Roadmap**][url.roadmap] - [vote on new features][url.vote_feature]


## Features

* ***Fast*** - Built in [Rust][url.rust], Vector is [fast and memory efficient][docs.performance]. No runtime. No garbage collector.
* **Correct** - Obsessed with [getting the details right][docs.correctness].
* **Vendor Neutral** - Does not favor a specific storage. Fair, open, with the user's best interest in mind.
* **Agent Or Service** - One simple tool to get data from A to B. Deploys as an [agent][docs.agent_role] or [service][docs.service_role].
* **Logs, Metrics, or Events** - Logs, metrics, and events. Collect, unify, and ship all observability data.
* **Clear Guarantees** - A [guarantee support matrix][docs.guarantees] helps you understand your tradeoffs.
* **Easy To Deploy** - Cross-compiles to a single static binary with no runtime.
* **Hot Reload** - [Reload configuration on the fly][docs.reloading], without skipping a beat.


## Performance

| Test | Vector | Filebeat | FluentBit | FluentD | Logstash | SplunkUF | SplunkHF |
| ---: | :---: | :---: | :---: | :---: | :---: | :---: | :---: |
| [TCP to Blackhole](https://github.com/timberio/vector-test-harness/tree/master/cases/tcp_to_blackhole_performance) | _**86mib/s**_ | n/a | 64.4mib/s | 27.7mib/s | 40.6mib/s | n/a | n/a |
| [File to TCP](https://github.com/timberio/vector-test-harness/tree/master/cases/file_to_tcp_performance) | _**76.7mib/s**_ | 7.8mib/s | 35mib/s | 26.1mib/s | 3.1mib/s | 40.1mib/s | 39mib/s |
| [Regex Parsing](https://github.com/timberio/vector-test-harness/tree/master/cases/regex_parsing_performance) | 13.2mib/s | n/a | _**20.5mib/s**_ | 2.6mib/s | 4.6mib/s | n/a | 7.8mib/s |
| [TCP to HTTP](https://github.com/timberio/vector-test-harness/tree/master/cases/tcp_to_http_performance) | _**26.7mib/s**_ | n/a | 19.6mib/s | <1mib/s | 2.7mib/s | n/a | n/a |
| [TCP to TCP](https://github.com/timberio/vector-test-harness/tree/master/cases/tcp_to_tcp_performance) | 69.9mib/s | 5mib/s | 67.1mib/s | 3.9mib/s | 10mib/s | _**70.4mib/s**_ | 7.6mib/s |

To learn more about our performance tests, please see the [Vector test harness][url.test_harness].


## Correctness

| Test | Vector | Filebeat | FluentBit | FluentD | Logstash | Splunk UF | Splunk HF |
| ---: | :---: | :---: | :---: | :---: | :---: | :---: | :---: |
| [Disk Buffer Persistence](https://github.com/timberio/vector-test-harness/tree/master/cases/disk_buffer_persistence_correctness) | ✅ | ✅ | ❌ | ❌ | ⚠️ | ✅ | ✅ |
| [File Rotate (create)](https://github.com/timberio/vector-test-harness/tree/master/cases/file_rotate_create_correctness) | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| [File Rotate (copytruncate)](https://github.com/timberio/vector-test-harness/tree/master/cases/file_rotate_truncate_correctness) | ✅ | ❌ | ❌ | ❌ | ❌ | ✅ | ✅ |
| [File Truncation](https://github.com/timberio/vector-test-harness/tree/master/cases/file_truncate_correctness) | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| [Process (SIGHUP)](https://github.com/timberio/vector-test-harness/tree/master/cases/sighup_correctness) | ✅ | ❌ | ❌ | ❌ | ⚠️ | ✅ | ✅ |
| [JSON (wrapped)](https://github.com/timberio/vector-test-harness/tree/master/cases/wrapped_json_correctness) | ✅ | ✅ | ❌ | ✅ | ✅ | ✅ | ✅ |

To learn more about our performance tests, please see the [Vector test harness][url.test_harness].


## Installation

Run the following in your terminal, then follow the on-screen instructions.

```bash
curl https://sh.vector.dev -sSf | sh
```

Or view [platform specific installation instructions][docs.installation].


## Sources

| Name  | Description |
|:------|:------------|
| [**`file`**][docs.file_source] | Ingests data through one or more local files and outputs [`log`][docs.log_event] events. |
| [**`statsd`**][docs.statsd_source] | Ingests data through the StatsD UDP protocol and outputs [`log`][docs.log_event] events. |
| [**`stdin`**][docs.stdin_source] | Ingests data through standard input (STDIN) and outputs [`log`][docs.log_event] events. |
| [**`syslog`**][docs.syslog_source] | Ingests data through the Syslog 5424 protocol and outputs [`log`][docs.log_event] events. |
| [**`tcp`**][docs.tcp_source] | Ingests data through the TCP protocol and outputs [`log`][docs.log_event] events. |
| [**`vector`**][docs.vector_source] | Ingests data through another upstream Vector instance and outputs [`log`][docs.log_event] events. |

[+ request a new source][url.new_source]


## Transforms

| Name  | Description |
|:------|:------------|
| [**`add_fields`**][docs.add_fields_transform] | Accepts [`log`][docs.log_event] events and allows you to add one or more fields. |
| [**`field_filter`**][docs.field_filter_transform] | Accepts [`log`][docs.log_event] and [`metric`][docs.metric_event] events and allows you to filter events by a field's value. |
| [**`grok_parser`**][docs.grok_parser_transform] | Accepts [`log`][docs.log_event] events and allows you to parse a field value with [Grok][url.grok]. |
| [**`json_parser`**][docs.json_parser_transform] | Accepts [`log`][docs.log_event] events and allows you to parse a field value as JSON. |
| [**`log_to_metric`**][docs.log_to_metric_transform] | Accepts [`log`][docs.log_event] events and allows you to convert logs into one or more metrics. |
| [**`lua`**][docs.lua_transform] | Accepts [`log`][docs.log_event] events and allows you to transform events with a full embedded [Lua][url.lua] engine. |
| [**`regex_parser`**][docs.regex_parser_transform] | Accepts [`log`][docs.log_event] events and allows you to parse a field's value with a [Regular Expression][url.regex]. |
| [**`remove_fields`**][docs.remove_fields_transform] | Accepts [`log`][docs.log_event] and [`metric`][docs.metric_event] events and allows you to remove one or more event fields. |
| [**`sampler`**][docs.sampler_transform] | Accepts [`log`][docs.log_event] events and allows you to sample events with a configurable rate. |
| [**`tokenizer`**][docs.tokenizer_transform] | Accepts [`log`][docs.log_event] events and allows you to tokenize a field's value by splitting on white space, ignoring special wrapping characters, and zipping the tokens into ordered field names. |

[+ request a new transform][url.new_transform]


## Sinks

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


[docs.add_fields_transform]: /usage/configuration/transforms/add_fields.md
[docs.administration]: /usage/administration
[docs.agent_role]: /setup/deployment/roles/agent.md
[docs.apt]: /setup/installation/package-managers/apt.md
[docs.aws_cloudwatch_logs_sink]: /usage/configuration/sinks/aws_cloudwatch_logs.md
[docs.aws_kinesis_streams_sink]: /usage/configuration/sinks/aws_kinesis_streams.md
[docs.aws_s3_sink]: /usage/configuration/sinks/aws_s3.md
[docs.blackhole_sink]: /usage/configuration/sinks/blackhole.md
[docs.concepts]: /about/concepts.md
[docs.configuration]: /usage/configuration
[docs.console_sink]: /usage/configuration/sinks/console.md
[docs.correctness]: /correctness.md
[docs.data_model]: /about/data-model.md
[docs.deployment]: /setup/deployment
[docs.docker]: /setup/installation/platforms/docker.md
[docs.elasticsearch_sink]: /usage/configuration/sinks/elasticsearch.md
[docs.field_filter_transform]: /usage/configuration/transforms/field_filter.md
[docs.file_source]: /usage/configuration/sources/file.md
[docs.getting_started]: /setup/getting-started
[docs.grok_parser_transform]: /usage/configuration/transforms/grok_parser.md
[docs.guarantees]: /about/guarantees.md
[docs.guides]: /usage/guides
[docs.homebrew]: /setup/installation/package-managers/homebrew.md
[docs.http_sink]: /usage/configuration/sinks/http.md
[docs.installation]: /setup/installation
[docs.json_parser_transform]: /usage/configuration/transforms/json_parser.md
[docs.kafka_sink]: /usage/configuration/sinks/kafka.md
[docs.log_event]: /about/data-model.md#log
[docs.log_to_metric_transform]: /usage/configuration/transforms/log_to_metric.md
[docs.lua_transform]: /usage/configuration/transforms/lua.md
[docs.metric_event]: /about/data-model.md#metric
[docs.performance]: /performance.md
[docs.prometheus_sink]: /usage/configuration/sinks/prometheus.md
[docs.regex_parser_transform]: /usage/configuration/transforms/regex_parser.md
[docs.reloading]: /usage/administration/reloading.md
[docs.remove_fields_transform]: /usage/configuration/transforms/remove_fields.md
[docs.roles]: /setup/deployment/roles
[docs.sampler_transform]: /usage/configuration/transforms/sampler.md
[docs.service_role]: /setup/deployment/roles/service.md
[docs.sinks]: /usage/configuration/sinks
[docs.sources]: /usage/configuration/sources
[docs.splunk_hec_sink]: /usage/configuration/sinks/splunk_hec.md
[docs.starting]: /usage/administration/starting.md
[docs.statsd_source]: /usage/configuration/sources/statsd.md
[docs.stdin_source]: /usage/configuration/sources/stdin.md
[docs.stopping]: /usage/administration/stopping.md
[docs.syslog_source]: /usage/configuration/sources/syslog.md
[docs.tcp_sink]: /usage/configuration/sinks/tcp.md
[docs.tcp_source]: /usage/configuration/sources/tcp.md
[docs.tokenizer_transform]: /usage/configuration/transforms/tokenizer.md
[docs.topologies]: /setup/deployment/topologies.md
[docs.transforms]: /usage/configuration/transforms
[docs.updating]: /usage/administration/updating.md
[docs.use_cases]: /use-cases
[docs.vector_sink]: /usage/configuration/sinks/vector.md
[docs.vector_source]: /usage/configuration/sources/vector.md
[docs.yum]: /setup/installation/package-managers/yum.md
[url.aws_cw_logs]: https://docs.aws.amazon.com/AmazonCloudWatch/latest/logs/WhatIsCloudWatchLogs.html
[url.aws_kinesis_data_streams]: https://aws.amazon.com/kinesis/data-streams/
[url.aws_s3]: https://aws.amazon.com/s3/
[url.community]: https://vector.dev/community
[url.elasticsearch]: https://www.elastic.co/products/elasticsearch
[url.grok]: http://grokdebug.herokuapp.com/
[url.kafka]: https://kafka.apache.org/
[url.kafka_protocol]: https://kafka.apache.org/protocol
[url.lua]: https://www.lua.org/
[url.mailing_list]: https://vector.dev/mailing_list/
[url.new_sink]: https://github.com/timberio/vector/issues/new?labels=Type%3A+New+Feature
[url.new_source]: https://github.com/timberio/vector/issues/new?labels=Type%3A+New+Feature
[url.new_transform]: https://github.com/timberio/vector/issues/new?labels=Type%3A+New+Feature
[url.prometheus]: https://prometheus.io/
[url.regex]: https://en.wikipedia.org/wiki/Regular_expression
[url.roadmap]: https://github.com/timberio/vector/milestones?direction=asc&sort=title&state=open
[url.rust]: https://www.rust-lang.org/
[url.splunk_hec]: http://dev.splunk.com/view/event-collector/SP-CAAAE6M
[url.test_harness]: https://github.com/timberio/vector-test-harness/
[url.vector_chat]: https://chat.vector.dev
[url.vector_forum]: https://forum.vector.dev
[url.vote_feature]: https://github.com/timberio/vector/issues?q=is%3Aissue+is%3Aopen+sort%3Areactions-%2B1-desc+label%3A%22Type%3A+New+Feature%22
