---
date: "2021-07-21"
title: "0.16 Upgrade Guide"
description: "An upgrade guide that addresses breaking changes in 0.16.0"
authors: ["jszwedko"]
pr_numbers: []
release: "0.16.0"
hide_on_release_notes: false
badges:
  type: breaking change
---

Vector's 0.16.0 release includes one breaking change:

1. [Datadog Log sink encoding option removed](#encoding)
1. [Renaming of `memory_use_bytes` internal metric](#memory_use_bytes)

We cover it below to help you upgrade quickly:

## Upgrade Guide

### Datadog Log sink encoding option removed {#encoding}

In previous versions of vector it was possible to configure the Datadog logs
sink to send in 'text' or 'json' encoding. While the logs ingest API does accept
text format the native format for that API is json. Sending text comes with
limitations and is only useful for backward compatability with older clients.

We no longer allow you to set the encoding of the payloads in the Datadog logs
sink. For instance, if your configuration looks like so:

```toml
[sinks.dd_logs_egress]
type = "datadog_logs"
inputs = ["datadog_agent"]
encoding.codec = "json"
```

You should remove `encoding.codec` entirely, leaving you with:

```toml
[sinks.dd_logs_egress]
type = "datadog_logs"
inputs = ["datadog_agent"]
```

Encoding fields other than `codec` are still valid.

### Renaming of `memory_use_bytes` internal metric {#memory_use_bytes}

Vector previously documented the `internal_metrics` `memory_use_bytes` metric as
being "The total memory currently being used by Vector (in bytes)."; however,
this metric was actually published by the `lua` transform and indicated the
memory use of just the Lua runtime.

To make this more clear, the metric has been renamed from `memory_use_bytes` to
`lua_memory_use_bytes`. If you were previously using `memory_use_bytes` as
a measure of the `lua` runtime memory usage, you should update to refer to
`lua_memory_use_bytes`.

The documentation for this metric has also been updated.
