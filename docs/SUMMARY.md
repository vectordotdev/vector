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
    * [file source][docs.sources.file]
    * [journald source][docs.sources.journald]
    * [kafka source][docs.sources.kafka]
    * [statsd source][docs.sources.statsd]
    * [stdin source][docs.sources.stdin]
    * [syslog source][docs.sources.syslog]
    * [tcp source][docs.sources.tcp]
    * [udp source][docs.sources.udp]
    * [vector source][docs.sources.vector]
  * [Transforms](usage/configuration/transforms/README.md)
    * [add_fields transform][docs.transforms.add_fields]
    * [add_tags transform][docs.transforms.add_tags]
    * [coercer transform][docs.transforms.coercer]
    * [field_filter transform][docs.transforms.field_filter]
    * [grok_parser transform][docs.transforms.grok_parser]
    * [json_parser transform][docs.transforms.json_parser]
    * [log_to_metric transform][docs.transforms.log_to_metric]
    * [lua transform][docs.transforms.lua]
    * [regex_parser transform][docs.transforms.regex_parser]
    * [remove_fields transform][docs.transforms.remove_fields]
    * [remove_tags transform][docs.transforms.remove_tags]
    * [sampler transform][docs.transforms.sampler]
    * [split transform][docs.transforms.split]
    * [tokenizer transform][docs.transforms.tokenizer]
  * [Sinks](usage/configuration/sinks/README.md)
    * [aws_cloudwatch_logs sink][docs.sinks.aws_cloudwatch_logs]
    * [aws_kinesis_streams sink][docs.sinks.aws_kinesis_streams]
    * [aws_s3 sink][docs.sinks.aws_s3]
    * [blackhole sink][docs.sinks.blackhole]
    * [clickhouse sink][docs.sinks.clickhouse]
    * [console sink][docs.sinks.console]
    * [elasticsearch sink][docs.sinks.elasticsearch]
    * [file sink][docs.sinks.file]
    * [http sink][docs.sinks.http]
    * [kafka sink][docs.sinks.kafka]
    * [prometheus sink][docs.sinks.prometheus]
    * [splunk_hec sink][docs.sinks.splunk_hec]
    * [tcp sink][docs.sinks.tcp]
    * [vector sink][docs.sinks.vector]
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


[docs.sinks.aws_cloudwatch_logs]: ./usage/configuration/sinks/aws_cloudwatch_logs.md
[docs.sinks.aws_kinesis_streams]: ./usage/configuration/sinks/aws_kinesis_streams.md
[docs.sinks.aws_s3]: ./usage/configuration/sinks/aws_s3.md
[docs.sinks.blackhole]: ./usage/configuration/sinks/blackhole.md
[docs.sinks.clickhouse]: ./usage/configuration/sinks/clickhouse.md
[docs.sinks.console]: ./usage/configuration/sinks/console.md
[docs.sinks.elasticsearch]: ./usage/configuration/sinks/elasticsearch.md
[docs.sinks.file]: ./usage/configuration/sinks/file.md
[docs.sinks.http]: ./usage/configuration/sinks/http.md
[docs.sinks.kafka]: ./usage/configuration/sinks/kafka.md
[docs.sinks.prometheus]: ./usage/configuration/sinks/prometheus.md
[docs.sinks.splunk_hec]: ./usage/configuration/sinks/splunk_hec.md
[docs.sinks.tcp]: ./usage/configuration/sinks/tcp.md
[docs.sinks.vector]: ./usage/configuration/sinks/vector.md
[docs.sources.file]: ./usage/configuration/sources/file.md
[docs.sources.journald]: ./usage/configuration/sources/journald.md
[docs.sources.kafka]: ./usage/configuration/sources/kafka.md
[docs.sources.statsd]: ./usage/configuration/sources/statsd.md
[docs.sources.stdin]: ./usage/configuration/sources/stdin.md
[docs.sources.syslog]: ./usage/configuration/sources/syslog.md
[docs.sources.tcp]: ./usage/configuration/sources/tcp.md
[docs.sources.udp]: ./usage/configuration/sources/udp.md
[docs.sources.vector]: ./usage/configuration/sources/vector.md
[docs.transforms.add_fields]: ./usage/configuration/transforms/add_fields.md
[docs.transforms.add_tags]: ./usage/configuration/transforms/add_tags.md
[docs.transforms.coercer]: ./usage/configuration/transforms/coercer.md
[docs.transforms.field_filter]: ./usage/configuration/transforms/field_filter.md
[docs.transforms.grok_parser]: ./usage/configuration/transforms/grok_parser.md
[docs.transforms.json_parser]: ./usage/configuration/transforms/json_parser.md
[docs.transforms.log_to_metric]: ./usage/configuration/transforms/log_to_metric.md
[docs.transforms.lua]: ./usage/configuration/transforms/lua.md
[docs.transforms.regex_parser]: ./usage/configuration/transforms/regex_parser.md
[docs.transforms.remove_fields]: ./usage/configuration/transforms/remove_fields.md
[docs.transforms.remove_tags]: ./usage/configuration/transforms/remove_tags.md
[docs.transforms.sampler]: ./usage/configuration/transforms/sampler.md
[docs.transforms.split]: ./usage/configuration/transforms/split.md
[docs.transforms.tokenizer]: ./usage/configuration/transforms/tokenizer.md
