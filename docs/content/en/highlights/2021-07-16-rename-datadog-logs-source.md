---
date: "2021-07-16"
title: "The `datadog_logs` source has been renamed to `datadog_agent`"
description: "To make the intention of the `datadog_logs` source clearer it has been renamed to `datadog_agent`"
authors: ["jszweko"]
pr_numbers: [8350]
release: "0.16.0"
hide_on_release_notes: false
badges:
  type: "breaking"
  providers: ["datadog"]
  domains: ["config"]
---

With the release of Vector 0.16.0, we've renamed the `datadog_logs` source to `datadog_agent`.

The naming of the `datadog_logs` source was somewhat ambiguous as it could be construed to indicate it is compatible
with the `datadog_logs` sink and that it mimics the [Datadog Logs API][datadog_logs_api]. However, the intention of this
source is to collect data specifically from running [Datadog Agents][datadog_agent] and this release contains some more
baked in assumptions that the data is specifically coming from agent.

For now, this source only collects logs forwarded by the agent, but in the future it will be expanded to collect metrics
and traces.

It is possible that we will re-add a `datadog_logs` source in the future that mimics the Datadog API for use with other
Datadog clients aside from the agent. Let us know if this would be useful to you!

We decided to make this a breaking change, instead of aliasing `datadog_logs`, as the released changes are not backwords
compatible and the name change reflects this.

## Upgrade Guide

Rename an `datadog_logs` source components in your configuration to `datadog_agent`:

```diff
[sources.datadog]
-type = "datadog_logs"
+type = "datadog_agent"
address = "0.0.0.0:8080"
store_api_key = true
```

[datadog_agent]: https://docs.datadoghq.com/agent/
[datadog_logs_api]: https://docs.datadoghq.com/api/latest/logs/#send-logs
