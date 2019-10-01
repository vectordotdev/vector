# Table of contents

* [What Is Vector?](README.md)
* [Use Cases][docs.use-cases.readme]
  * [Reduce Lock-In](use-cases/lock-in.md)
  * [Multi-Cloud](use-cases/multi-cloud.md)
  * [Governance & Control](use-cases/governance.md)
  * [Reduce Cost](use-cases/cost.md)
  * [Security & Compliance](use-cases/security-and-compliance.md)
  * [Backups & Archives](use-cases/backups.md)
* [Performance][docs.performance]
* [Correctness][docs.correctness]

## About

* [Concepts][docs.concepts]
* [Data Model][docs.data-model.readme]
  * [Log Event][docs.data-model.log]
  * [Metric Event][docs.data-model.metric]
* [Guarantees][docs.guarantees]

## Setup

* [Installation][docs.installation.readme]
  * [Platforms][docs.installation.platforms.readme]
    * [Docker][docs.platforms.docker]
  * [Package Managers][docs.installation.package-managers.readme]
    * [APT][docs.package-managers.apt]
    * [Homebrew][docs.package-managers.homebrew]
    * [YUM][docs.package-managers.yum]
  * [Operating Systems][docs.installation.operating-systems.readme]
    * [Amazon Linux][docs.operating-systems.amazon-linux]
    * [CentOS][docs.operating-systems.centos]
    * [Debian][docs.operating-systems.debian]
    * [MacOS][docs.operating-systems.macos]
    * [RHEL][docs.operating-systems.rhel]
    * [Ubuntu][docs.operating-systems.ubuntu]
  * [Manual][docs.installation.manual]
    * [From Archives][docs.from-archives]
    * [From Source][docs.from-source]
* [Getting Started][docs.getting-started.readme]
  * [Sending Your First Event][docs.sending-your-first-event]
* [Deployment][docs.deployment.readme]
  * [Topologies][docs.deployment.topologies]
  * [Roles][docs.roles.readme]
    * [Agent Role][docs.roles.agent]
    * [Service Role][docs.roles.service]

## Usage

* [Configuration](usage/configuration/README.md)
  * [Sources](usage/configuration/sources/README.md)
    * [file source](usage/configuration/sources/file.md)
    * [journald source](usage/configuration/sources/journald.md)
    * [statsd source](usage/configuration/sources/statsd.md)
    * [stdin source](usage/configuration/sources/stdin.md)
    * [syslog source](usage/configuration/sources/syslog.md)
    * [tcp source](usage/configuration/sources/tcp.md)
    * [udp source](usage/configuration/sources/udp.md)
    * [vector source](usage/configuration/sources/vector.md)
  * [Transforms](usage/configuration/transforms/README.md)
    * [add\_fields transform](usage/configuration/transforms/add_fields.md)
    * [coercer transform](usage/configuration/transforms/coercer.md)
    * [field\_filter transform](usage/configuration/transforms/field_filter.md)
    * [grok\_parser transform](usage/configuration/transforms/grok_parser.md)
    * [javascript transform](usage/configuration/transforms/javascript.md)
    * [json\_parser transform](usage/configuration/transforms/json_parser.md)
    * [log\_to\_metric transform](usage/configuration/transforms/log_to_metric.md)
    * [lua transform](usage/configuration/transforms/lua.md)
    * [regex\_parser transform](usage/configuration/transforms/regex_parser.md)
    * [remove\_fields transform](usage/configuration/transforms/remove_fields.md)
    * [sampler transform](usage/configuration/transforms/sampler.md)
    * [tokenizer transform](usage/configuration/transforms/tokenizer.md)
  * [Sinks](usage/configuration/sinks/README.md)
    * [aws\_cloudwatch\_logs sink](usage/configuration/sinks/aws_cloudwatch_logs.md)
    * [aws\_kinesis\_streams sink](usage/configuration/sinks/aws_kinesis_streams.md)
    * [aws\_s3 sink](usage/configuration/sinks/aws_s3.md)
    * [blackhole sink](usage/configuration/sinks/blackhole.md)
    * [clickhouse sink](usage/configuration/sinks/clickhouse.md)
    * [console sink](usage/configuration/sinks/console.md)
    * [elasticsearch sink](usage/configuration/sinks/elasticsearch.md)
    * [http sink](usage/configuration/sinks/http.md)
    * [kafka sink](usage/configuration/sinks/kafka.md)
    * [prometheus sink](usage/configuration/sinks/prometheus.md)
    * [splunk\_hec sink](usage/configuration/sinks/splunk_hec.md)
    * [tcp sink](usage/configuration/sinks/tcp.md)
    * [vector sink](usage/configuration/sinks/vector.md)
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

* [Changelog][urls.vector_changelog]
* [Community][urls.vector_community]
* [Downloads][urls.vector_downloads]
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
[docs.concepts]: ./about/concepts.md
[docs.configuration.readme]: ./usage/configuration/README.md
[docs.configuration.specification]: ./usage/configuration/specification.md
[docs.conventions]: ./meta/conventions.md
[docs.correctness]: ./correctness.md
[docs.data-model.log]: ./about/data-model/log.md
[docs.data-model.metric]: ./about/data-model/metric.md
[docs.data-model.readme]: ./about/data-model/README.md
[docs.deployment.readme]: ./setup/deployment/README.md
[docs.deployment.topologies]: ./setup/deployment/topologies.md
[docs.from-archives]: ./setup/installation/manual/from-archives.md
[docs.from-source]: ./setup/installation/manual/from-source.md
[docs.getting-started.readme]: ./setup/getting-started/README.md
[docs.glossary]: ./meta/glossary.md
[docs.guarantees]: ./about/guarantees.md
[docs.guides.troubleshooting]: ./usage/guides/troubleshooting.md
[docs.guides]: ./usage/guides
[docs.installation.manual]: ./setup/installation/manual
[docs.installation.operating-systems.readme]: ./setup/installation/operating-systems/README.md
[docs.installation.package-managers.readme]: ./setup/installation/package-managers/README.md
[docs.installation.platforms.readme]: ./setup/installation/platforms/README.md
[docs.installation.readme]: ./setup/installation/README.md
[docs.operating-systems.amazon-linux]: ./setup/installation/operating-systems/amazon-linux.md
[docs.operating-systems.centos]: ./setup/installation/operating-systems/centos.md
[docs.operating-systems.debian]: ./setup/installation/operating-systems/debian.md
[docs.operating-systems.macos]: ./setup/installation/operating-systems/macos.md
[docs.operating-systems.rhel]: ./setup/installation/operating-systems/rhel.md
[docs.operating-systems.ubuntu]: ./setup/installation/operating-systems/ubuntu.md
[docs.package-managers.apt]: ./setup/installation/package-managers/apt.md
[docs.package-managers.homebrew]: ./setup/installation/package-managers/homebrew.md
[docs.package-managers.yum]: ./setup/installation/package-managers/yum.md
[docs.performance]: ./performance.md
[docs.platforms.docker]: ./setup/installation/platforms/docker.md
[docs.roles.agent]: ./setup/deployment/roles/agent.md
[docs.roles.readme]: ./setup/deployment/roles/README.md
[docs.roles.service]: ./setup/deployment/roles/service.md
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
[docs.sinks.readme]: ./usage/configuration/sinks/README.md
[docs.sinks.splunk_hec]: ./usage/configuration/sinks/splunk_hec.md
[docs.sinks.statsd]: ./usage/configuration/sinks/statsd.md
[docs.sinks.tcp]: ./usage/configuration/sinks/tcp.md
[docs.sinks.vector]: ./usage/configuration/sinks/vector.md
[docs.sources.file]: ./usage/configuration/sources/file.md
[docs.sources.journald]: ./usage/configuration/sources/journald.md
[docs.sources.kafka]: ./usage/configuration/sources/kafka.md
[docs.sources.readme]: ./usage/configuration/sources/README.md
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
[docs.transforms.readme]: ./usage/configuration/transforms/README.md
[docs.transforms.regex_parser]: ./usage/configuration/transforms/regex_parser.md
[docs.transforms.remove_fields]: ./usage/configuration/transforms/remove_fields.md
[docs.transforms.remove_tags]: ./usage/configuration/transforms/remove_tags.md
[docs.transforms.sampler]: ./usage/configuration/transforms/sampler.md
[docs.transforms.split]: ./usage/configuration/transforms/split.md
[docs.transforms.tokenizer]: ./usage/configuration/transforms/tokenizer.md
[docs.use-cases.readme]: ./use-cases/README.md
[urls.vector_changelog]: https://github.com/timberio/vector/blob/master/CHANGELOG.md
[urls.vector_community]: https://vector.dev/community
[urls.vector_downloads]: https://packages.timber.io/vector
[urls.vector_releases]: https://github.com/timberio/vector/releases
[urls.vector_repo]: https://github.com/timberio/vector
[urls.vector_roadmap]: https://github.com/timberio/vector/milestones?direction=asc&sort=due_date&state=open
