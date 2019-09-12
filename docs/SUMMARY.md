# Table of contents

* [What Is Vector?](README.md)
* [Use Cases](use-cases/README.md)
  * [Reduce Lock-In](use-cases/lock-in.md)
  * [Multi-Cloud](use-cases/multi-cloud.md)
  * [Governance & Control](use-cases/governance.md)
  * [Reduce Cost](use-cases/cost.md)
  * [Security & Compliance](use-cases/security-and-compliance.md)
  * [Backups & Archives](use-cases/backups.md)
* [Performance](performance.md)
* [Correctness](correctness.md)

## About

* [Concepts](about/concepts.md)
* [Data Model](about/data-model/README.md)
  * [Log Event](about/data-model/log.md)
  * [Metric Event](about/data-model/metric.md)
* [Guarantees](about/guarantees.md)

## Setup

* [Installation](setup/installation/README.md)
  * [Platforms](setup/installation/platforms/README.md)
    * [Docker](setup/installation/platforms/docker.md)
  * [Package Managers](setup/installation/package-managers/README.md)
    * [APT](setup/installation/package-managers/apt.md)
    * [Homebrew](setup/installation/package-managers/homebrew.md)
    * [YUM](setup/installation/package-managers/yum.md)
  * [Operating Systems](setup/installation/operating-systems/README.md)
    * [Amazon Linux](setup/installation/operating-systems/amazon-linux.md)
    * [CentOS](setup/installation/operating-systems/centos.md)
    * [Debian](setup/installation/operating-systems/debian.md)
    * [MacOS](setup/installation/operating-systems/macos.md)
    * [RHEL](setup/installation/operating-systems/rhel.md)
    * [Ubuntu](setup/installation/operating-systems/ubuntu.md)
  * [Manual](setup/installation/manual/README.md)
    * [From Archives](setup/installation/manual/from-archives.md)
    * [From Source](setup/installation/manual/from-source.md)
* [Getting Started](setup/getting-started/README.md)
  * [Sending Your First Event](setup/getting-started/sending-your-first-event.md)
* [Deployment](setup/deployment/README.md)
  * [Topologies](setup/deployment/topologies.md)
  * [Roles](setup/deployment/roles/README.md)
    * [Agent Role](setup/deployment/roles/agent.md)
    * [Service Role](setup/deployment/roles/service.md)

## Usage

* [Configuration](usage/configuration/README.md)
  * [Sources](usage/configuration/sources/README.md)
    * [file source][docs.file_source]
    * [journald source][docs.journald_source]
    * [kafka source][docs.kafka_source]
    * [statsd source][docs.statsd_source]
    * [stdin source][docs.stdin_source]
    * [syslog source][docs.syslog_source]
    * [tcp source][docs.tcp_source]
    * [udp source][docs.udp_source]
    * [vector source][docs.vector_source]
  * [Transforms](usage/configuration/transforms/README.md)
    * [add_fields transform][docs.add_fields_transform]
    * [add_tags transform][docs.add_tags_transform]
    * [coercer transform][docs.coercer_transform]
    * [field_filter transform][docs.field_filter_transform]
    * [grok_parser transform][docs.grok_parser_transform]
    * [json_parser transform][docs.json_parser_transform]
    * [log_to_metric transform][docs.log_to_metric_transform]
    * [lua transform][docs.lua_transform]
    * [regex_parser transform][docs.regex_parser_transform]
    * [remove_fields transform][docs.remove_fields_transform]
    * [remove_tags transform][docs.remove_tags_transform]
    * [sampler transform][docs.sampler_transform]
    * [tokenizer transform][docs.tokenizer_transform]
  * [Sinks](usage/configuration/sinks/README.md)
    * [aws_cloudwatch_logs sink][docs.aws_cloudwatch_logs_sink]
    * [aws_kinesis_streams sink][docs.aws_kinesis_streams_sink]
    * [aws_s3 sink][docs.aws_s3_sink]
    * [blackhole sink][docs.blackhole_sink]
    * [clickhouse sink][docs.clickhouse_sink]
    * [console sink][docs.console_sink]
    * [elasticsearch sink][docs.elasticsearch_sink]
    * [file sink][docs.file_sink]
    * [http sink][docs.http_sink]
    * [kafka sink][docs.kafka_sink]
    * [prometheus sink][docs.prometheus_sink]
    * [splunk_hec sink][docs.splunk_hec_sink]
    * [tcp sink][docs.tcp_sink]
    * [vector sink][docs.vector_sink]
  * [Specification](usage/configuration/specification.md)
* [Administration](usage/administration/README.md)
  * [Starting](usage/administration/starting.md)
  * [Reloading](usage/administration/reloading.md)
  * [Stopping](usage/administration/stopping.md)
  * [Monitoring](usage/administration/monitoring.md)
  * [Tuning](usage/administration/tuning.md)
  * [Updating](usage/administration/updating.md)
  * [Validating](usage/administration/validating.md)
  * [Env Vars](usage/administration/env-vars.md)
* [Guides](usage/guides/README.md)
  * [Troubleshooting Guide](usage/guides/troubleshooting.md)

## Resources

* [Community](https://vector.dev/community/)
* [Download](https://github.com/timberio/vector/releases)
* [Github Repo](https://github.com/timberio/vector)
* [Roadmap](https://github.com/timberio/vector/milestones?direction=asc&sort=title&state=open)

## Meta

* [Conventions](meta/conventions.md)
* [Glossary](meta/glossary.md)


[docs.add_fields_transform]: ./usage/configuration/transforms/add_fields.md
[docs.add_tags_transform]: ./usage/configuration/transforms/add_tags.md
[docs.aws_cloudwatch_logs_sink]: ./usage/configuration/sinks/aws_cloudwatch_logs.md
[docs.aws_kinesis_streams_sink]: ./usage/configuration/sinks/aws_kinesis_streams.md
[docs.aws_s3_sink]: ./usage/configuration/sinks/aws_s3.md
[docs.blackhole_sink]: ./usage/configuration/sinks/blackhole.md
[docs.clickhouse_sink]: ./usage/configuration/sinks/clickhouse.md
[docs.coercer_transform]: ./usage/configuration/transforms/coercer.md
[docs.console_sink]: ./usage/configuration/sinks/console.md
[docs.elasticsearch_sink]: ./usage/configuration/sinks/elasticsearch.md
[docs.field_filter_transform]: ./usage/configuration/transforms/field_filter.md
[docs.file_sink]: ./usage/configuration/sinks/file.md
[docs.file_source]: ./usage/configuration/sources/file.md
[docs.grok_parser_transform]: ./usage/configuration/transforms/grok_parser.md
[docs.http_sink]: ./usage/configuration/sinks/http.md
[docs.journald_source]: ./usage/configuration/sources/journald.md
[docs.json_parser_transform]: ./usage/configuration/transforms/json_parser.md
[docs.kafka_sink]: ./usage/configuration/sinks/kafka.md
[docs.kafka_source]: ./usage/configuration/sources/kafka.md
[docs.log_to_metric_transform]: ./usage/configuration/transforms/log_to_metric.md
[docs.lua_transform]: ./usage/configuration/transforms/lua.md
[docs.prometheus_sink]: ./usage/configuration/sinks/prometheus.md
[docs.regex_parser_transform]: ./usage/configuration/transforms/regex_parser.md
[docs.remove_fields_transform]: ./usage/configuration/transforms/remove_fields.md
[docs.remove_tags_transform]: ./usage/configuration/transforms/remove_tags.md
[docs.sampler_transform]: ./usage/configuration/transforms/sampler.md
[docs.splunk_hec_sink]: ./usage/configuration/sinks/splunk_hec.md
[docs.statsd_source]: ./usage/configuration/sources/statsd.md
[docs.stdin_source]: ./usage/configuration/sources/stdin.md
[docs.syslog_source]: ./usage/configuration/sources/syslog.md
[docs.tcp_sink]: ./usage/configuration/sinks/tcp.md
[docs.tcp_source]: ./usage/configuration/sources/tcp.md
[docs.tokenizer_transform]: ./usage/configuration/transforms/tokenizer.md
[docs.udp_source]: ./usage/configuration/sources/udp.md
[docs.vector_sink]: ./usage/configuration/sinks/vector.md
[docs.vector_source]: ./usage/configuration/sources/vector.md
