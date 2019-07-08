---
description: Receive and pull log and metric events into Vector
---

<!--
     THIS FILE IS AUTOOGENERATED!

     To make changes please edit the template located at:

     scripts/generate/templates/docs/usage/configuration/sources/README.md.erb
-->

# Sources

![][images.sources]

Sources are responsible for ingesting [events][docs.event] into Vector, they can
both receive and pull in data. If you're deploying Vector in an [agent
role][docs.agent_role], you'll want to look at local data sources like a
[`file`][docs.file_source] and [`stdin`][docs.stdin_source]. If you're deploying
Vector in a [service role][docs.service_role], you'll want to look at sources
that receive data over the network, like the [`vector`][docs.vector_source],
[`tcp`][docs.tcp_source], and [`syslog`][docs.syslog_source] sources.

| Name  | Description |
|:------|:------------|
| [**`file`**][docs.file_source] | Ingests data through one or more local files and outputs [`log`][docs.log_event] events. |
| [**`statsd`**][docs.statsd_source] | Ingests data through the StatsD UDP protocol and outputs [`log`][docs.log_event] events. |
| [**`stdin`**][docs.stdin_source] | Ingests data through standard input (STDIN) and outputs [`log`][docs.log_event] events. |
| [**`syslog`**][docs.syslog_source] | Ingests data through the Syslog 5424 protocol and outputs [`log`][docs.log_event] events. |
| [**`tcp`**][docs.tcp_source] | Ingests data through the TCP protocol and outputs [`log`][docs.log_event] events. |
| [**`vector`**][docs.vector_source] | Ingests data through another upstream Vector instance and outputs [`log`][docs.log_event] events. |

[+ request a new source][url.new_source]


[docs.agent_role]: https://docs.vector.dev/setup/deployment/roles/agent
[docs.event]: https://docs.vector.dev/about/data-model#event
[docs.file_source]: https://docs.vector.dev/usage/configuration/sources/file
[docs.log_event]: https://docs.vector.dev/about/data-model#log
[docs.service_role]: https://docs.vector.dev/setup/deployment/roles/service
[docs.statsd_source]: https://docs.vector.dev/usage/configuration/sources/statsd
[docs.stdin_source]: https://docs.vector.dev/usage/configuration/sources/stdin
[docs.syslog_source]: https://docs.vector.dev/usage/configuration/sources/syslog
[docs.tcp_source]: https://docs.vector.dev/usage/configuration/sources/tcp
[docs.vector_source]: https://docs.vector.dev/usage/configuration/sources/vector
[images.sources]: https://docs.vector.dev/assets/sources.svg
[url.new_source]: https://github.com/timberio/vector/issues/new?labels=Type%3A+New+Feature
