<p align="center">
  <strong>
    <a href="https://vector.dev/docs/setup/quickstart/">Quickstart</a>&nbsp;&nbsp;&bull;&nbsp;&nbsp;
    <a href="https://vector.dev/docs/">Docs</a>&nbsp;&nbsp;&bull;&nbsp;&nbsp;
    <a href="https://vector.dev/guides/">Guides</a>&nbsp;&nbsp;&bull;&nbsp;&nbsp;
    <a href="https://vector.dev/components/">Integrations</a>&nbsp;&nbsp;&bull;&nbsp;&nbsp;
    <a href="https://chat.vector.dev">Chat</a>&nbsp;&nbsp;&bull;&nbsp;&nbsp;
    <a href="https://vector.dev/releases/latest/download/">Download</a>&nbsp;&nbsp;&bull;&nbsp;&nbsp;
    <a href="https://rust-doc.vector.dev/">Rust Crate Docs</a>
  </strong>
</p>
<p align="center">
  <img src="website/static/img/diagram.svg" alt="Vector">
</p>

## What is Vector?

Vector is a high-performance, end-to-end (agent & aggregator) observability data
pipeline that puts you in control of your observability data.
[Collect][docs.sources], [transform][docs.transforms], and [route][docs.sinks]
all your logs and metrics to any vendors you want today and any other
vendors you may want tomorrow. Vector enables dramatic cost reduction, novel
data enrichment, and data security where you need it, not where it is most
convenient for your vendors. Additionally, it is open source and up to 10x
faster than every alternative in the space.

To get started, follow our [**quickstart guide**][docs.quickstart] or [**install
Vector**][docs.installation].

### Principles

* **Reliable** - Built in [Rust][urls.rust], Vector's primary design goal is reliability.
* **End-to-end** - Deploys as an [agent][docs.roles#agent] or [aggregator][docs.roles#aggregator]. Vector is a complete platform.
* **Unified** - [Logs][docs.data-model.log], [metrics][docs.data-model.metric] (beta), and traces (coming soon). One tool for all of your data.

### Use cases

* Reduce total observability costs.
* Transition vendors without disrupting workflows.
* Enhance data quality and improve insights.
* Consolidate agents and eliminate agent fatigue.
* Improve overall observability performance and reliability.

### Community

* Vector is relied on by startups and enterprises like **Atlassian**, **T-Mobile**,
  **Comcast**, **Zendesk**, **Discord**, **Fastly**, **CVS**, **Trivago**,
  **Tuple**, **Douban**, **Visa**, **Mambu**, **Blockfi**, **Claranet**,
  **Instacart**, **Forcepoint**, and [many more][urls.production_users].
* Vector is **downloaded over 100,000 times per day**.
* Vector's largest user **processes over 30TB daily**.
* Vector has **over 100 contributors** and growing.

## [Documentation](https://vector.dev/docs/)

### About

* [**Concepts**][docs.about.concepts]
* [**Under the hood**][docs.about.under-the-hood]
  * [**Architecture**][docs.under-the-hood.architecture] - [data model][docs.architecture.data-model] ([log][docs.data-model.log], [metric][docs.data-model.metric]), [pipeline model][docs.architecture.pipeline-model], [concurrency model][docs.architecture.concurrency-model], [runtime model][docs.architecture.runtime-model]
  * [**Networking**][docs.under-the-hood.networking] - [ARC][docs.networking.adaptive-request-concurrency]
  * [**Guarantees**][docs.under-the-hood.guarantees]

### Setup

* [**Quickstart**][docs.setup.quickstart]
* [**Installation**][docs.setup.installation] - [operating systems][docs.installation.operating_systems], [package managers][docs.installation.package_managers], [platforms][docs.installation.platforms] ([Kubernetes][docs.platforms.kubernetes]), [manual][docs.installation.manual]
* [**Deployment**][docs.deployment] - [roles][docs.deployment.roles], [topologies][docs.deployment.topologies]

### Reference

* **Configuration**
  * [**Sources**][docs.configuration.sources] - [docker_logs][docs.sources.docker_logs], [file][docs.sources.file], [http][docs.sources.http], [journald][docs.sources.journald], [kafka][docs.sources.kafka], [socket][docs.sources.socket], and [many more...][docs.sources]
  * [**Transforms**][docs.configuration.transforms] - [dedupe][docs.transforms.dedupe], [filter][docs.transforms.filter], [geoip][docs.transforms.geoip], [log_to_metric][docs.transforms.log_to_metric], [lua][docs.transforms.lua], [remap][docs.transforms.remap], and [many more...][docs.transforms]
  * [**Sinks**][docs.configuration.sinks] - [aws_cloudwatch_logs][docs.sinks.aws_cloudwatch_logs], [aws_s3][docs.sinks.aws_s3], [clickhouse][docs.sinks.clickhouse], [elasticsearch][docs.sinks.elasticsearch], [gcp_cloud_storage][docs.sinks.gcp_cloud_storage], and [many more...][docs.sinks]
  * [**Unit tests**][docs.configuration.tests]
* [**Remap Language**][docs.reference.vrl]
* [**API**][docs.reference.api]
* [**CLI**][docs.reference.cli]

### Administration

* [**Management**][docs.administration.management]
* [**Monitoring & observing**][docs.administration.monitoring]
* [**Upgrading**][docs.administration.upgrading]
* [**Validating**][docs.administration.validating]

### Resources

* [**Community**][urls.vector_community] - [chat][urls.vector_chat], [calendar][urls.vector_calendar], [@vectordotdev][urls.vector_twitter]
* [**Releases**][urls.vector_releases]
* **Policies** - [Code of Conduct][urls.vector_code_of_conduct], [Privacy][urls.vector_privacy_policy], [Releases][urls.vector_releases_policy], [Security][urls.vector_security_policy], [Versioning][urls.vector_versioning_policy]

## Comparisons

### Performance

The following performance tests demonstrate baseline performance between
common protocols with the exception of the Regex Parsing test.

|                                                                                                               Test |     Vector      | Filebeat |    FluentBit    |  FluentD  | Logstash  |    SplunkUF     | SplunkHF |
|-------------------------------------------------------------------------------------------------------------------:|:---------------:|:--------:|:---------------:|:---------:|:---------:|:---------------:|:--------:|
| [TCP to Blackhole](https://github.com/vectordotdev/vector-test-harness/tree/master/cases/tcp_to_blackhole_performance) |  _**86mib/s**_  |   n/a    |    64.4mib/s    | 27.7mib/s | 40.6mib/s |       n/a       |   n/a    |
|           [File to TCP](https://github.com/vectordotdev/vector-test-harness/tree/master/cases/file_to_tcp_performance) | _**76.7mib/s**_ | 7.8mib/s |     35mib/s     | 26.1mib/s | 3.1mib/s  |    40.1mib/s    | 39mib/s  |
|       [Regex Parsing](https://github.com/vectordotdev/vector-test-harness/tree/master/cases/regex_parsing_performance) |    13.2mib/s    |   n/a    | _**20.5mib/s**_ | 2.6mib/s  | 4.6mib/s  |       n/a       | 7.8mib/s |
|           [TCP to HTTP](https://github.com/vectordotdev/vector-test-harness/tree/master/cases/tcp_to_http_performance) | _**26.7mib/s**_ |   n/a    |    19.6mib/s    |  <1mib/s  | 2.7mib/s  |       n/a       |   n/a    |
|             [TCP to TCP](https://github.com/vectordotdev/vector-test-harness/tree/master/cases/tcp_to_tcp_performance) |    69.9mib/s    |  5mib/s  |    67.1mib/s    | 3.9mib/s  |  10mib/s  | _**70.4mib/s**_ | 7.6mib/s |

To learn more about our performance tests, please see the [Vector test harness][urls.vector_test_harness].

### Correctness

The following correctness tests are not exhaustive, but they demonstrate
fundamental differences in quality and attention to detail:

|                                                                                                                             Test | Vector | Filebeat | FluentBit | FluentD | Logstash | Splunk UF | Splunk HF |
|---------------------------------------------------------------------------------------------------------------------------------:|:------:|:--------:|:---------:|:-------:|:--------:|:---------:|:---------:|
| [Disk Buffer Persistence](https://github.com/vectordotdev/vector-test-harness/tree/master/cases/disk_buffer_persistence_correctness) | **✓**  |    ✓     |           |         |    ⚠     |     ✓     |     ✓     |
|         [File Rotate (create)](https://github.com/vectordotdev/vector-test-harness/tree/master/cases/file_rotate_create_correctness) | **✓**  |    ✓     |     ✓     |    ✓    |    ✓     |     ✓     |     ✓     |
| [File Rotate (copytruncate)](https://github.com/vectordotdev/vector-test-harness/tree/master/cases/file_rotate_truncate_correctness) | **✓**  |          |           |         |          |     ✓     |     ✓     |
|                   [File Truncation](https://github.com/vectordotdev/vector-test-harness/tree/master/cases/file_truncate_correctness) | **✓**  |    ✓     |     ✓     |    ✓    |    ✓     |     ✓     |     ✓     |
|                         [Process (SIGHUP)](https://github.com/vectordotdev/vector-test-harness/tree/master/cases/sighup_correctness) | **✓**  |          |           |         |    ⚠     |     ✓     |     ✓     |
|                     [JSON (wrapped)](https://github.com/vectordotdev/vector-test-harness/tree/master/cases/wrapped_json_correctness) | **✓**  |    ✓     |     ✓     |    ✓    |    ✓     |     ✓     |     ✓     |

To learn more about our correctness tests, please see the [Vector test harness][urls.vector_test_harness].

### Features

Vector is an end-to-end, unified, open data platform.

|                     | **Vector** | Beats | Fluentbit | Fluentd | Logstash | Splunk UF | Splunk HF | Telegraf |
|--------------------:|:----------:|:-----:|:---------:|:-------:|:--------:|:---------:|:---------:|:--------:|
|      **End-to-end** |   **✓**    |       |           |         |          |           |           |     ✓    |
|               Agent |   **✓**    |   ✓   |     ✓     |         |          |     ✓     |           |     ✓    |
|          Aggregator |   **✓**    |       |           |    ✓    |    ✓     |           |     ✓     |     ✓    |
|         **Unified** |   **✓**    |       |           |         |          |           |           |     ✓    |
|                Logs |   **✓**    |   ✓   |     ✓     |    ✓    |    ✓     |     ✓     |     ✓     |     ✓    |
|             Metrics |   **✓**    |   ⚠   |     ⚠     |    ⚠    |    ⚠     |     ⚠     |     ⚠     |     ✓    |
|              Traces |     🚧     |       |           |         |          |           |           |          |
|            **Open** |   **✓**    |       |     ✓     |    ✓    |          |           |           |     ✓    |
|         Open-source |   **✓**    |   ✓   |     ✓     |    ✓    |    ✓     |           |           |     ✓    |
|      Vendor-neutral |   **✓**    |       |     ✓     |    ✓    |          |           |           |     ✓    |
|     **Reliability** |   **✓**    |       |           |         |          |           |           |          |
|         Memory-safe |   **✓**    |       |           |         |          |           |           |     ✓    |
| Delivery guarantees |   **✓**    |       |           |         |          |     ✓     |     ✓     |          |
|          Multi-core |   **✓**    |   ✓   |     ✓     |    ✓    |    ✓     |     ✓     |     ✓     |     ✓    |


⚠ = Not interoperable, metrics are represented as structured logs

---

<p align="center">
  Developed with ❤️ by <strong><a href="https://datadoghq.com">Datadog</a></strong> - <a href="https://github.com/vectordotdev/vector/security/policy">Security Policy</a> - <a href="https://github.com/vectordotdev/vector/blob/master/PRIVACY.md">Privacy Policy</a>
</p>

[docs.about.concepts]: https://vector.dev/docs/about/concepts/
[docs.about.under-the-hood]: https://vector.dev/docs/about/under-the-hood/
[docs.administration.monitoring]: https://vector.dev/docs/administration/monitoring/
[docs.administration.management]: https://vector.dev/docs/administration/management/
[docs.administration.upgrading]: https://vector.dev/docs/administration/upgrading/
[docs.administration.validating]: https://vector.dev/docs/administration/validating/
[docs.architecture.concurrency-model]: https://vector.dev/docs/about/under-the-hood/architecture/concurrency-model/
[docs.architecture.data-model]: https://vector.dev/docs/about/under-the-hood/architecture/data-model/
[docs.architecture.pipeline-model]: https://vector.dev/docs/about/under-the-hood/architecture/pipeline-model/
[docs.architecture.runtime-model]: https://vector.dev/docs/about/under-the-hood/architecture/runtime-model/
[docs.configuration.sinks]: https://vector.dev/docs/reference/configuration/sinks/
[docs.configuration.sources]: https://vector.dev/docs/reference/configuration/sources/
[docs.configuration.tests]: https://vector.dev/docs/reference/configuration/tests/
[docs.configuration.transforms]: https://vector.dev/docs/reference/configuration/transforms/
[docs.data-model.log]: https://vector.dev/docs/about/under-the-hood/architecture/data-model/log/
[docs.data-model.metric]: https://vector.dev/docs/about/under-the-hood/architecture/data-model/metric/
[docs.deployment.roles]: https://vector.dev/docs/setup/deployment/roles/
[docs.deployment.topologies]: https://vector.dev/docs/setup/deployment/topologies/
[docs.deployment]: https://vector.dev/docs/setup/deployment/
[docs.installation.manual]: https://vector.dev/docs/setup/installation/manual/
[docs.installation.operating_systems]: https://vector.dev/docs/setup/installation/operating-systems/
[docs.installation.package_managers]: https://vector.dev/docs/setup/installation/package-managers/
[docs.installation.platforms]: https://vector.dev/docs/setup/installation/platforms/
[docs.installation]: https://vector.dev/docs/setup/installation/
[docs.networking.adaptive-request-concurrency]: https://vector.dev/docs/about/under-the-hood/networking/arc/
[docs.platforms.kubernetes]: https://vector.dev/docs/setup/installation/platforms/kubernetes/
[docs.quickstart]: https://vector.dev/docs/setup/quickstart/
[docs.reference.api]: https://vector.dev/docs/reference/api/
[docs.reference.cli]: https://vector.dev/docs/reference/cli/
[docs.reference.vrl]: https://vector.dev/docs/reference/vrl/
[docs.roles#agent]: https://vector.dev/docs/setup/deployment/roles/#agent
[docs.roles#aggregator]: https://vector.dev/docs/setup/deployment/roles/#aggregator
[docs.setup.installation]: https://vector.dev/docs/setup/installation/
[docs.setup.quickstart]: https://vector.dev/docs/setup/quickstart/
[docs.sinks.aws_cloudwatch_logs]: https://vector.dev/docs/reference/configuration/sinks/aws_cloudwatch_logs/
[docs.sinks.aws_s3]: https://vector.dev/docs/reference/configuration/sinks/aws_s3/
[docs.sinks.clickhouse]: https://vector.dev/docs/reference/configuration/sinks/clickhouse/
[docs.sinks.elasticsearch]: https://vector.dev/docs/reference/configuration/sinks/elasticsearch/
[docs.sinks.gcp_cloud_storage]: https://vector.dev/docs/reference/configuration/sinks/gcp_cloud_storage/
[docs.sinks]: https://vector.dev/docs/reference/configuration/sinks/
[docs.sources.docker_logs]: https://vector.dev/docs/reference/configuration/sources/docker_logs/
[docs.sources.file]: https://vector.dev/docs/reference/configuration/sources/file/
[docs.sources.http]: https://vector.dev/docs/reference/configuration/sources/http/
[docs.sources.journald]: https://vector.dev/docs/reference/configuration/sources/journald/
[docs.sources.kafka]: https://vector.dev/docs/reference/configuration/sources/kafka/
[docs.sources.socket]: https://vector.dev/docs/reference/configuration/sources/socket/
[docs.sources]: https://vector.dev/docs/reference/configuration/sources/
[docs.transforms.dedupe]: https://vector.dev/docs/reference/configuration/transforms/dedupe/
[docs.transforms.filter]: https://vector.dev/docs/reference/configuration/transforms/filter/
[docs.transforms.log_to_metric]: https://vector.dev/docs/reference/configuration/transforms/log_to_metric/
[docs.transforms.lua]: https://vector.dev/docs/reference/configuration/transforms/lua/
[docs.transforms.remap]: https://vector.dev/docs/reference/configuration/transforms/remap/
[docs.transforms]: https://vector.dev/docs/reference/configuration/transforms/
[docs.under-the-hood.architecture]: https://vector.dev/docs/about/under-the-hood/architecture/
[docs.under-the-hood.guarantees]: https://vector.dev/docs/about/under-the-hood/guarantees/
[docs.under-the-hood.networking]: https://vector.dev/docs/about/under-the-hood/networking/
[urls.production_users]: https://github.com/vectordotdev/vector/issues/790
[urls.rust]: https://www.rust-lang.org/
[urls.vector_calendar]: https://calendar.vector.dev
[urls.vector_chat]: https://chat.vector.dev
[urls.vector_code_of_conduct]: https://github.com/vectordotdev/vector/blob/master/CODE_OF_CONDUCT.md
[urls.vector_community]: https://vector.dev/community/
[urls.vector_privacy_policy]: https://github.com/vectordotdev/vector/blob/master/PRIVACY.md
[urls.vector_release_policy]: https://github.com/vectordotdev/vector/blob/master/RELEASING.md
[urls.vector_releases]: https://vector.dev/releases/
[urls.vector_releases_policy]: https://github.com/vectordotdev/vector/blob/master/RELEASES.md
[urls.vector_security_policy]: https://github.com/vectordotdev/vector/security/policy
[urls.vector_test_harness]: https://github.com/vectordotdev/vector-test-harness/
[urls.vector_twitter]: https://twitter.com/vectordotdev
[urls.vector_versioning_policy]: https://github.com/vectordotdev/vector/blob/master/VERSIONING.md
[urls.vote_feature]: https://github.com/vectordotdev/vector/issues?q=is%3Aissue+is%3Aopen+sort%3Areactions-%2B1-desc+label%3A%22type%3A+feature%22

