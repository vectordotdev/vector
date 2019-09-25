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
  * [Manual][docs.installation.manual]
    * [From Archives][docs.from-archives]
    * [From Source][docs.from-source]
* [Getting Started][docs.getting-started]
  * [Sending Your First Event][docs.sending-your-first-event]
* [Deployment][docs.deployment]
  * [Topologies][docs.deployment.topologies]
  * [Roles][docs.roles]
    * [Agent Role][docs.roles.agent]
    * [Service Role][docs.roles.service]

## Usage

* [Configuration][docs.configuration]
  * [Sources][docs.sources]
    * [file source][docs.sources.file]
    * [journald source][docs.sources.journald]
    * [kafka source][docs.sources.kafka]
    * [statsd source][docs.sources.statsd]
    * [stdin source][docs.sources.stdin]
    * [syslog source][docs.sources.syslog]
    * [tcp source][docs.sources.tcp]
    * [udp source][docs.sources.udp]
    * [vector source][docs.sources.vector]
  * [Transforms][docs.transforms]
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
  * [Sinks][docs.sinks]
    * [aws_cloudwatch_logs sink][docs.sinks.aws_cloudwatch_logs]
    * [aws_cloudwatch_metrics sink][docs.sinks.aws_cloudwatch_metrics]
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
    * [statsd sink][docs.sinks.statsd]
    * [tcp sink][docs.sinks.tcp]
    * [vector sink][docs.sinks.vector]
  * [Specification][docs.configuration.specification]
* [Administration][docs.administration]
  * [Starting][docs.administration.starting]
  * [Reloading][docs.administration.reloading]
  * [Stopping][docs.administration.stopping]
  * [Monitoring][docs.administration.monitoring]
  * [Tuning][docs.administration.tuning]
  * [Updating][docs.administration.updating]
  * [Validating][docs.administration.validating]
  * [Env Vars][docs.administration.env-vars]
* [Guides][docs.guides]
  * [Troubleshooting Guide][docs.guides.troubleshooting]

## Resources

* [Changelog][urls.vector_changelog]
* [Community][urls.vector_community]
* [Github Repo][urls.vector_repo]
* [Releases][urls.vector_releases]
* [Roadmap][urls.vector_roadmap]

## Meta

* [Conventions][docs.conventions]
* [Glossary][docs.glossary]


[docs.administration.env-vars]: ./usage/administration/env-vars.md
[docs.administration.monitoring]: ./usage/administration/monitoring.md
[docs.administration.reloading]: ./usage/administration/reloading.md
[docs.administration.starting]: ./usage/administration/starting.md
[docs.administration.stopping]: ./usage/administration/stopping.md
[docs.administration.tuning]: ./usage/administration/tuning.md
[docs.administration.updating]: ./usage/administration/updating.md
[docs.administration.validating]: ./usage/administration/validating.md
[docs.administration]: ./usage/administration
[docs.configuration.specification]: ./usage/configuration/specification.md
[docs.configuration]: ./usage/configuration
[docs.conventions]: ./meta/conventions.md
[docs.deployment.topologies]: ./setup/deployment/topologies.md
[docs.deployment]: ./setup/deployment
[docs.from-archives]: ./setup/installation/manual/from-archives.md
[docs.from-source]: ./setup/installation/manual/from-source.md
[docs.getting-started]: ./setup/getting-started
[docs.glossary]: ./meta/glossary.md
[docs.guides.troubleshooting]: ./usage/guides/troubleshooting.md
[docs.guides]: ./usage/guides
[docs.installation.manual]: ./setup/installation/manual
[docs.roles.agent]: ./setup/deployment/roles/agent.md
[docs.roles.service]: ./setup/deployment/roles/service.md
[docs.roles]: ./setup/deployment/roles
[docs.sending-your-first-event]: ./setup/getting-started/sending-your-first-event.md
[docs.sinks.aws_cloudwatch_logs]: ./usage/configuration/sinks/aws_cloudwatch_logs.md
[docs.sinks.aws_cloudwatch_metrics]: ./usage/configuration/sinks/aws_cloudwatch_metrics.md
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
[docs.sinks.statsd]: ./usage/configuration/sinks/statsd.md
[docs.sinks.tcp]: ./usage/configuration/sinks/tcp.md
[docs.sinks.vector]: ./usage/configuration/sinks/vector.md
[docs.sinks]: ./usage/configuration/sinks
[docs.sources.file]: ./usage/configuration/sources/file.md
[docs.sources.journald]: ./usage/configuration/sources/journald.md
[docs.sources.kafka]: ./usage/configuration/sources/kafka.md
[docs.sources.statsd]: ./usage/configuration/sources/statsd.md
[docs.sources.stdin]: ./usage/configuration/sources/stdin.md
[docs.sources.syslog]: ./usage/configuration/sources/syslog.md
[docs.sources.tcp]: ./usage/configuration/sources/tcp.md
[docs.sources.udp]: ./usage/configuration/sources/udp.md
[docs.sources.vector]: ./usage/configuration/sources/vector.md
[docs.sources]: ./usage/configuration/sources
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
[docs.transforms]: ./usage/configuration/transforms
[urls.vector_changelog]: https://github.com/timberio/vector/blob/master/CHANGELOG.md
[urls.vector_community]: https://vector.dev/community
[urls.vector_releases]: https://github.com/timberio/vector/releases
[urls.vector_repo]: https://github.com/timberio/vector
[urls.vector_roadmap]: https://github.com/timberio/vector/milestones?direction=asc&sort=due_date&state=open
