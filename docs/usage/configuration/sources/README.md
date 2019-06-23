---
description: Receive and pull log and metric events into Vector
---

# Sources

![](../../../assets/sources.svg)

Sources are responsible for ingesting [events][docs.event] into Vector, they can both receive and pull in data. If you're deploying Vector in an [agent role][docs.agent_role], you'll want to look at local data sources like a [`file`][docs.file_source] and [`stdin`][docs.stdin_source]. If you're deploying Vector in a [service role][docs.service_role], you'll want to look at sources that receive data over the network, like the [`vector`][docs.vector_source], [`tcp`][docs.tcp_source], and [`syslog`][docs.syslog_source] sources.

<!-- START: sources_table -->
<!-- ----------------------------------------------------------------- -->
<!-- DO NOT MODIFY! This section is generated via `make generate-docs` -->

| Name | Description |
| :--- | :---------- |
| [**`file`**][docs.file_source] | Ingests data through one or more local files and outputs [`log`][docs.log_event] events.<br />`guarantee: best_effort` |
| [**`statsd`**][docs.statsd_source] | Ingests data through the StatsD UDP protocol and outputs [`log`][docs.log_event] events.<br />`guarantee: best_effort` |
| [**`stdin`**][docs.stdin_source] | Ingests data through standard input (STDIN) and outputs [`log`][docs.log_event] events.<br />`guarantee: at_least_once` |
| [**`syslog`**][docs.syslog_source] | Ingests data through the Syslog 5424 protocol and outputs [`log`][docs.log_event] events.<br />`guarantee: best_effort` |
| [**`tcp`**][docs.tcp_source] | Ingests data through the TCP protocol and outputs [`log`][docs.log_event] events.<br />`guarantee: best_effort` |
| [**`vector`**][docs.vector_source] | Ingests data through another upstream Vector instance and outputs [`log`][docs.log_event] events.<br />`guarantee: best_effort` |

<!-- ----------------------------------------------------------------- -->
<!-- END: sources_table -->

[+ request a new source](https://github.com/timberio/vector/issues/new?labels=Type%3A+New+Feature%2C%7B%3Atitle%3D%3E%22New+%60%3Cname%3E%60+source%22%7D&title=New+%60%3Cname%3E%60+source)


[docs.agent_role]: ../../../setup/deployment/roles/agent.md
[docs.event]: ../../../about/data-model.md#event
[docs.file_source]: ../../../usage/configuration/sources/file.md
[docs.log_event]: ../../../about/data-model.md#log
[docs.service_role]: ../../../setup/deployment/roles/service.md
[docs.statsd_source]: ../../../usage/configuration/sources/statsd.md
[docs.stdin_source]: ../../../usage/configuration/sources/stdin.md
[docs.syslog_source]: ../../../usage/configuration/sources/syslog.md
[docs.tcp_source]: ../../../usage/configuration/sources/tcp.md
[docs.vector_source]: ../../../usage/configuration/sources/vector.md
