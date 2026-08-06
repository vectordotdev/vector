---
date: "2020-04-13"
title: "New Datadog Logs Sink"
description: "Sink logs to the Datadog logging service"
authors: ["binarylogic"]
pr_numbers: [1832]
release: "0.9.0"
hide_on_release_notes: true
badges:
  type: "new feature"
  domains: ["sinks"]
  sinks: ["datadog_logs"]
---

In addition to our [`datadog_metrics` sink][docs.sinks.datadog_metrics], we've
introduced a new [`datadog_logs` sink][docs.sinks.datadog_logs]. This is part
of our broader effort to expand Vector's integrations.

[docs.sinks.datadog_logs]: /docs/reference/configuration/sinks/datadog_logs/
[docs.sinks.datadog_metrics]: /docs/reference/configuration/sinks/datadog_metrics/
