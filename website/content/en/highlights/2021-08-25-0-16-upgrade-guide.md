---
date: "2021-08-25"
title: "0.16 Upgrade Guide"
description: "An upgrade guide that addresses breaking changes in 0.16.0"
authors: ["jszwedko", "JeanMertz", "spencergilbert"]
pr_numbers: []
release: "0.16.0"
hide_on_release_notes: false
badges:
  type: breaking change
---

Vector's 0.16.0 release includes three **breaking changes**:

1. [Component name field renamed to ID](#name-to-id)
1. [Datadog Log sink encoding option removed](#encoding)
1. [Renaming of `memory_use_bytes` internal metric](#memory_use_bytes)
1. [`datadog_logs` source renamed to `datadog_agent`](#datadog_logs_rename)
1. [`kubernetes_logs` source's new RBAC](#kubernetes_logs_rbac)

We cover them below to help you upgrade quickly:

## Upgrade guide

### Component name field renamed to ID {#name-to-id}

Historically we've referred to the component ID field as `name` in some places, `id` in others. We've decided to
standardize on `ID` as we feel this is closer to the intention of the field: an unchanging identifier for
components.

For example, with the component config:

```toml
[transforms.parse_nginx]
type = "remap"
inputs = []
source = ""
```

The `parse_nginx` part of the config is now only referred to as `ID` in the documentation.

We have preserved compatibility with existing usages of `component_name` for the `internal_metrics` sources by keeping
`component_name` and adding `component_id` as a new tag. However, we recommend switching usages over to `component_id`
as we will be removing `component_name` in the future: if you were grouping by this tag in your metrics queries, or
referring to it in a `remap` or `lua` transform, you should update it to refer to `component_id`.

Within the GraphQL API, all references to `name` for `Component`s has been updated to be `componentId`. This is used
over simply `Id` as `Id` has special semantics within the GraphQL ecosystem and we may add support for this field later.

### Datadog Log sink encoding option removed {#encoding}

In previous versions of vector it was possible to configure the Datadog logs
sink to send in 'text' or 'json' encoding. While the logs ingest API does accept
text format the native format for that API is json. Sending text comes with
limitations and is only useful for backward compatibility with older clients.

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

### `datadog_logs` source renamed to `datadog_agent` {#datadog_logs_rename}

With the release of Vector 0.16.0, we've renamed the `datadog_logs` source to `datadog_agent`.

The naming of the `datadog_logs` source was somewhat ambiguous as it could be construed to indicate it is compatible
with the `datadog_logs` sink and that it mimics the [Datadog Logs API][datadog_logs_api]. However, the intention of this
source is to collect data specifically from running [Datadog Agents][datadog_agent] and this release contains some more
baked in assumptions that the data is specifically coming from agent.

For now, this source only collects logs forwarded by the agent, but in the future it will be expanded to collect metrics
and traces.

We decided to make this a breaking change, instead of aliasing `datadog_logs`, as the released changes are not backwards
compatible and the name change reflects this.

It is possible that we will re-add a `datadog_logs` source in the future that mimics the Datadog API for use with other
Datadog clients aside from the agent. Let us know if this would be useful to you!

### `kubernetes_logs` source's new RBAC {#kubernetes_logs_rbac}

The `kubernetes_logs` source will now enrich events with labels from the Namespace they originate from. This enhancement
requires access to an additional resource in Kubernetes. Our Kubernetes manifests and Helm chart have been updated to
create a `ClusterRole` granting access to the `namespaces` resource.

## Upgrade Guide

Rename a `datadog_logs` source components in your configuration to `datadog_agent`:

```diff
[sources.datadog]
-type = "datadog_logs"
+type = "datadog_agent"
address = "0.0.0.0:8080"
store_api_key = true
```

Updating to 0.16.0 requires that you update to the equivalent chart or Kubernetes manifests. If you don't use either of
our provided installation methods, you should update your `ClusterRole` as such:

```diff
apiVersion: rbac.authorization.k8s.io/v1
kind: ClusterRole
metadata:
  name: vector-agent
rules:
  - apiGroups:
      - ""
    resources:
+     - namespaces
      - pods
    verbs:
      - watch
```

[datadog_agent]: https://docs.datadoghq.com/agent/
[datadog_logs_api]: https://docs.datadoghq.com/api/latest/logs/#send-logs
