<p align="center">
  <strong>
    <a href="https://chat.vector.dev">Chat/Forum<a/>&nbsp;&nbsp;&bull;&nbsp;&nbsp;<a href="https://vector.dev/mailing_list">Mailing List<a/>&nbsp;&nbsp;&bull;&nbsp;&nbsp;<a href="https://docs.vector.dev/setup/installation">Install 0.4.0<a/>
  </strong>
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

Built in [Rust][urls.rust], Vector places high-value on
[performance][docs.performance], [correctness][docs.correctness], and [operator
friendliness][docs.administration]. It compiles to a single static binary and is
designed to be [deployed][docs.deployment] across your entire infrastructure,
serving both as a light-weight [agent][docs.roles.agent] and a highly efficient
[service][docs.roles.service], making the process of getting data from A to B
simple and unified.


## [Documentation](https://docs.vector.dev/)

#### About

* [**Use cases**][docs.use_cases]
* [**Concepts**][docs.concepts]
* [**Data model**][docs.data_model] - [log event][docs.data-model.log], [metric event][docs.data-model.metric]
* [**Guarantees**][docs.guarantees]

#### Setup

* [**Installation**][docs.installation] - [platforms][docs.platforms], [operating systems][docs.operating_systems], [package managers][docs.package_managers], [from archives][docs.from-archives], [from source][docs.from-source]
* [**Getting started**][docs.getting_started]
* [**Deployment**][docs.deployment] - [topologies][docs.topologies], [roles][docs.roles]

#### Usage

* [**Configuration**][docs.configuration] - [sources][docs.sources], [transforms][docs.transforms], [sinks][docs.sinks]
* [**Administration**][docs.administration] - [starting][docs.starting], [stopping][docs.stopping], [reloading][docs.reloading], [updating][docs.updating]
* [**Guides**][docs.guides]

#### Resources

* [**Community**][urls.vector_community] - [chat/forum][urls.vector_chat], [mailing list][urls.mailing_list]
* [**Releases**][urls.vector_releases] - [v0.4.0][urls.v0.4.0], [changelog][urls.vector_changelog]
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
* **Hot Reload** - [Reload configuration on the fly][docs.reloading], without skipping a beat.


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
curl https://sh.vector.dev -sSf | sh
```

Or view [platform specific installation instructions][docs.installation].


## Sources

| Name  | Description |
|:------|:------------|
| [**`file`**][docs.sources.file] | Ingests data through one or more local files and outputs [`log`][docs.data-model.log] events. |
| [**`journald`**][docs.sources.journald] | Ingests data through log records from journald and outputs [`log`][docs.data-model.log] events. |
| [**`kafka`**][docs.sources.kafka] | Ingests data through Kafka 0.9 or later and outputs [`log`][docs.data-model.log] events. |
| [**`statsd`**][docs.sources.statsd] | Ingests data through the StatsD UDP protocol and outputs [`metric`][docs.data-model.metric] events. |
| [**`stdin`**][docs.sources.stdin] | Ingests data through standard input (STDIN) and outputs [`log`][docs.data-model.log] events. |
| [**`syslog`**][docs.sources.syslog] | Ingests data through the Syslog 5424 protocol and outputs [`log`][docs.data-model.log] events. |
| [**`tcp`**][docs.sources.tcp] | Ingests data through the TCP protocol and outputs [`log`][docs.data-model.log] events. |
| [**`udp`**][docs.sources.udp] | Ingests data through the UDP protocol and outputs [`log`][docs.data-model.log] events. |
| [**`vector`**][docs.sources.vector] | Ingests data through another upstream Vector instance and outputs [`log`][docs.data-model.log] and [`metric`][docs.data-model.metric] events. |

[+ request a new source][urls.new_source]


## Transforms

| Name  | Description |
|:------|:------------|
| [**`add_fields`**][docs.transforms.add_fields] | Accepts [`log`][docs.data-model.log] events and allows you to add one or more log fields. |
| [**`add_tags`**][docs.transforms.add_tags] | Accepts [`metric`][docs.data-model.metric] events and allows you to add one or more metric tags. |
| [**`coercer`**][docs.transforms.coercer] | Accepts [`log`][docs.data-model.log] events and allows you to coerce log fields into fixed types. |
| [**`field_filter`**][docs.transforms.field_filter] | Accepts [`log`][docs.data-model.log] and [`metric`][docs.data-model.metric] events and allows you to filter events by a log field's value. |
| [**`grok_parser`**][docs.transforms.grok_parser] | Accepts [`log`][docs.data-model.log] events and allows you to parse a log field value with [Grok][urls.grok]. |
| [**`json_parser`**][docs.transforms.json_parser] | Accepts [`log`][docs.data-model.log] events and allows you to parse a log field value as JSON. |
| [**`log_to_metric`**][docs.transforms.log_to_metric] | Accepts [`log`][docs.data-model.log] events and allows you to convert logs into one or more metrics. |
| [**`lua`**][docs.transforms.lua] | Accepts [`log`][docs.data-model.log] events and allows you to transform events with a full embedded [Lua][urls.lua] engine. |
| [**`regex_parser`**][docs.transforms.regex_parser] | Accepts [`log`][docs.data-model.log] events and allows you to parse a log field's value with a [Regular Expression][urls.regex]. |
| [**`remove_fields`**][docs.transforms.remove_fields] | Accepts [`log`][docs.data-model.log] events and allows you to remove one or more log fields. |
| [**`remove_tags`**][docs.transforms.remove_tags] | Accepts [`metric`][docs.data-model.metric] events and allows you to remove one or more metric tags. |
| [**`sampler`**][docs.transforms.sampler] | Accepts [`log`][docs.data-model.log] events and allows you to sample events with a configurable rate. |
| [**`split`**][docs.transforms.split] | Accepts [`log`][docs.data-model.log] events and allows you to split a field's value on a given separator and zip the tokens into ordered field names. |
| [**`tokenizer`**][docs.transforms.tokenizer] | Accepts [`log`][docs.data-model.log] events and allows you to tokenize a field's value by splitting on white space, ignoring special wrapping characters, and zip the tokens into ordered field names. |

[+ request a new transform][urls.new_transform]


## Sinks

| Name  | Description |
|:------|:------------|
| [**`aws_cloudwatch_logs`**][docs.sinks.aws_cloudwatch_logs] | [Batches](#buffers-and-batches) [`log`][docs.data-model.log] events to [AWS CloudWatch Logs][urls.aws_cw_logs] via the [`PutLogEvents` API endpoint](https://docs.aws.amazon.com/AmazonCloudWatchLogs/latest/APIReference/API_PutLogEvents.html). |
| [**`aws_kinesis_streams`**][docs.sinks.aws_kinesis_streams] | [Batches](#buffers-and-batches) [`log`][docs.data-model.log] events to [AWS Kinesis Data Stream][urls.aws_kinesis_data_streams] via the [`PutRecords` API endpoint](https://docs.aws.amazon.com/kinesis/latest/APIReference/API_PutRecords.html). |
| [**`aws_s3`**][docs.sinks.aws_s3] | [Batches](#buffers-and-batches) [`log`][docs.data-model.log] events to [AWS S3][urls.aws_s3] via the [`PutObject` API endpoint](https://docs.aws.amazon.com/AmazonS3/latest/API/RESTObjectPUT.html). |
| [**`blackhole`**][docs.sinks.blackhole] | [Streams](#streaming) [`log`][docs.data-model.log] and [`metric`][docs.data-model.metric] events to a blackhole that simply discards data, designed for testing and benchmarking purposes. |
| [**`clickhouse`**][docs.sinks.clickhouse] | [Batches](#buffers-and-batches) [`log`][docs.data-model.log] events to [Clickhouse][urls.clickhouse] via the [`HTTP` Interface][urls.clickhouse_http]. |
| [**`console`**][docs.sinks.console] | [Streams](#streaming) [`log`][docs.data-model.log] and [`metric`][docs.data-model.metric] events to the console, `STDOUT` or `STDERR`. |
| [**`elasticsearch`**][docs.sinks.elasticsearch] | [Batches](#buffers-and-batches) [`log`][docs.data-model.log] events to [Elasticsearch][urls.elasticsearch] via the [`_bulk` API endpoint](https://www.elastic.co/guide/en/elasticsearch/reference/current/docs-bulk.html). |
| [**`file`**][docs.sinks.file] | [Streams](#streaming) [`log`][docs.data-model.log] events to a file. |
| [**`http`**][docs.sinks.http] | [Batches](#buffers-and-batches) [`log`][docs.data-model.log] events to a generic HTTP endpoint. |
| [**`kafka`**][docs.sinks.kafka] | [Streams](#streaming) [`log`][docs.data-model.log] events to [Apache Kafka][urls.kafka] via the [Kafka protocol][urls.kafka_protocol]. |
| [**`prometheus`**][docs.sinks.prometheus] | [Exposes](#exposing-and-scraping) [`metric`][docs.data-model.metric] events to [Prometheus][urls.prometheus] metrics service. |
| [**`splunk_hec`**][docs.sinks.splunk_hec] | [Batches](#buffers-and-batches) [`log`][docs.data-model.log] events to a [Splunk HTTP Event Collector][urls.splunk_hec]. |
| [**`tcp`**][docs.sinks.tcp] | [Streams](#streaming) [`log`][docs.data-model.log] events to a TCP connection. |
| [**`vector`**][docs.sinks.vector] | [Streams](#streaming) [`log`][docs.data-model.log] events to another downstream Vector instance. |

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


[docs.administration]: https://docs.vector.dev/usage/administration
[docs.archives]: https://docs.vector.dev/setup/installation/manual/from-archives
[docs.concepts]: https://docs.vector.dev/about/concepts
[docs.configuration]: https://docs.vector.dev/usage/configuration
[docs.correctness]: https://docs.vector.dev/correctness
[docs.data-model.log]: https://docs.vector.dev/about/data-model/log
[docs.data-model.metric]: https://docs.vector.dev/about/data-model/metric
[docs.data_model]: https://docs.vector.dev/about/data-model
[docs.deployment]: https://docs.vector.dev/setup/deployment
[docs.from-archives]: https://docs.vector.dev/setup/installation/manual/from-archives
[docs.from-source]: https://docs.vector.dev/setup/installation/manual/from-source
[docs.getting_started]: https://docs.vector.dev/setup/getting-started
[docs.guarantees]: https://docs.vector.dev/about/guarantees
[docs.guides]: https://docs.vector.dev/usage/guides
[docs.installation]: https://docs.vector.dev/setup/installation
[docs.operating_systems]: https://docs.vector.dev/setup/installation/operating-systems
[docs.package_managers]: https://docs.vector.dev/setup/installation/package-managers
[docs.performance]: https://docs.vector.dev/performance
[docs.platforms]: https://docs.vector.dev/setup/installation/platforms
[docs.reloading]: https://docs.vector.dev/usage/administration/reloading
[docs.roles.agent]: https://docs.vector.dev/setup/deployment/roles/agent
[docs.roles.service]: https://docs.vector.dev/setup/deployment/roles/service
[docs.roles]: https://docs.vector.dev/setup/deployment/roles
[docs.sinks.aws_cloudwatch_logs]: https://docs.vector.dev/usage/configuration/sinks/aws_cloudwatch_logs
[docs.sinks.aws_kinesis_streams]: https://docs.vector.dev/usage/configuration/sinks/aws_kinesis_streams
[docs.sinks.aws_s3]: https://docs.vector.dev/usage/configuration/sinks/aws_s3
[docs.sinks.blackhole]: https://docs.vector.dev/usage/configuration/sinks/blackhole
[docs.sinks.clickhouse]: https://docs.vector.dev/usage/configuration/sinks/clickhouse
[docs.sinks.console]: https://docs.vector.dev/usage/configuration/sinks/console
[docs.sinks.elasticsearch]: https://docs.vector.dev/usage/configuration/sinks/elasticsearch
[docs.sinks.file]: https://docs.vector.dev/usage/configuration/sinks/file
[docs.sinks.http]: https://docs.vector.dev/usage/configuration/sinks/http
[docs.sinks.kafka]: https://docs.vector.dev/usage/configuration/sinks/kafka
[docs.sinks.prometheus]: https://docs.vector.dev/usage/configuration/sinks/prometheus
[docs.sinks.splunk_hec]: https://docs.vector.dev/usage/configuration/sinks/splunk_hec
[docs.sinks.tcp]: https://docs.vector.dev/usage/configuration/sinks/tcp
[docs.sinks.vector]: https://docs.vector.dev/usage/configuration/sinks/vector
[docs.sinks]: https://docs.vector.dev/usage/configuration/sinks
[docs.sources.file]: https://docs.vector.dev/usage/configuration/sources/file
[docs.sources.journald]: https://docs.vector.dev/usage/configuration/sources/journald
[docs.sources.kafka]: https://docs.vector.dev/usage/configuration/sources/kafka
[docs.sources.statsd]: https://docs.vector.dev/usage/configuration/sources/statsd
[docs.sources.stdin]: https://docs.vector.dev/usage/configuration/sources/stdin
[docs.sources.syslog]: https://docs.vector.dev/usage/configuration/sources/syslog
[docs.sources.tcp]: https://docs.vector.dev/usage/configuration/sources/tcp
[docs.sources.udp]: https://docs.vector.dev/usage/configuration/sources/udp
[docs.sources.vector]: https://docs.vector.dev/usage/configuration/sources/vector
[docs.sources]: https://docs.vector.dev/usage/configuration/sources
[docs.starting]: https://docs.vector.dev/usage/administration/starting
[docs.stopping]: https://docs.vector.dev/usage/administration/stopping
[docs.topologies]: https://docs.vector.dev/setup/deployment/topologies
[docs.transforms.add_fields]: https://docs.vector.dev/usage/configuration/transforms/add_fields
[docs.transforms.add_tags]: https://docs.vector.dev/usage/configuration/transforms/add_tags
[docs.transforms.coercer]: https://docs.vector.dev/usage/configuration/transforms/coercer
[docs.transforms.field_filter]: https://docs.vector.dev/usage/configuration/transforms/field_filter
[docs.transforms.grok_parser]: https://docs.vector.dev/usage/configuration/transforms/grok_parser
[docs.transforms.json_parser]: https://docs.vector.dev/usage/configuration/transforms/json_parser
[docs.transforms.log_to_metric]: https://docs.vector.dev/usage/configuration/transforms/log_to_metric
[docs.transforms.lua]: https://docs.vector.dev/usage/configuration/transforms/lua
[docs.transforms.regex_parser]: https://docs.vector.dev/usage/configuration/transforms/regex_parser
[docs.transforms.remove_fields]: https://docs.vector.dev/usage/configuration/transforms/remove_fields
[docs.transforms.remove_tags]: https://docs.vector.dev/usage/configuration/transforms/remove_tags
[docs.transforms.sampler]: https://docs.vector.dev/usage/configuration/transforms/sampler
[docs.transforms.split]: https://docs.vector.dev/usage/configuration/transforms/split
[docs.transforms.tokenizer]: https://docs.vector.dev/usage/configuration/transforms/tokenizer
[docs.transforms]: https://docs.vector.dev/usage/configuration/transforms
[docs.updating]: https://docs.vector.dev/usage/administration/updating
[docs.use_cases]: https://docs.vector.dev/use-cases
[urls.aws_cw_logs]: https://docs.aws.amazon.com/AmazonCloudWatch/latest/logs/WhatIsCloudWatchLogs.html
[urls.aws_kinesis_data_streams]: https://aws.amazon.com/kinesis/data-streams/
[urls.aws_s3]: https://aws.amazon.com/s3/
[urls.clickhouse]: https://clickhouse.yandex/
[urls.clickhouse_http]: https://clickhouse.yandex/docs/en/interfaces/http/
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
[urls.test_harness]: https://github.com/timberio/vector-test-harness/
[urls.v0.4.0]: https://github.com/timberio/vector/releases/tag/v0.4.0
[urls.vector_changelog]: https://github.com/timberio/vector/blob/master/CHANGELOG.md
[urls.vector_chat]: https://chat.vector.dev
[urls.vector_community]: https://vector.dev/community
[urls.vector_releases]: https://github.com/timberio/vector/releases
[urls.vector_roadmap]: https://github.com/timberio/vector/milestones?direction=asc&sort=due_date&state=open
[urls.vote_feature]: https://github.com/timberio/vector/issues?q=is%3Aissue+is%3Aopen+sort%3Areactions-%2B1-desc+label%3A%22Type%3A+New+Feature%22
