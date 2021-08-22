---
date: "2021-07-28"
title: "0.16 Upgrade Guide"
description: "An upgrade guide that addresses breaking changes in 0.16.0"
authors: ["jszwedko", "JeanMertz"]
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

And one **deprecation**:

1. [Vector source/sink version 2 released](#vector_source_sink)

We cover them below to help you upgrade quickly:

## Upgrade guide

### Component name field renamed to ID {#name-to-id}

Historically we've referred to the component ID field as `name` in some places, `id` in others. We've decided to
standardize on `ID` as we feel this is more closer to the intention of the field: an unchanging identifier for
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
`component_name` and adding `component_id` as a new tag. Howover, wwe recommend switching usages over to `component_id`
as we will be removing `component_name` in the future: if you were grouping by this tag in your metrics queries, or
referring to it in a `remap` or `lua` transform, you should update it to refer to `component_id`.

Within the GraphQL API, all references to `name` for `Component`s has been updated to be `componentId`. This is used
over simply `Id` as `Id` has special semantics within the GraphQL ecosystem and we may add support for this field later.

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

### Vector source/sink version 2 released {#vector_source_sink}

We've released a new major version (`v2`) of our `vector` [source][]/[sink][]
components. This release resolves several issues and limitations we experienced
with our previous (`v1`) TCP-based implementation of these two components:

- `vector` sink does not work in k8s with dynamic IP addresses ([#2070][])
- Allow for HTTP in the vector source and sinks ([#5124][])
- Allow Vector Source and Sink to Communicate over GRPC ([#6646][])
- RFC 5843 - Encoding/Decoding for Vector to Vector Communication ([#6032][])

The new version transitions to using gRPC over HTTP as its communication
protocol, which resolves those limitations.

To allow operators to transition at their leisure, this new release of Vector
still defaults to `v1`. In the next release (`0.17.0`) we'll require operators
to explicitly state which version they want to use, but continue to support
`v1`. The release after that (`0.18.0`) we'll drop `v1` completely, and default
to `v2`, we also no longer require you to explicitly set the version since there
will only be one supported going forward.

If you want to opt in to the new (stable!) `v2` version, you can do so as
follows:

```diff
[sinks.vector]
  type = "vector"
+ version = "v2"

[sources.vector]
  type = "vector"
+ version = "v2"
```

There are a couple of things to be aware of:

#### Upgrade both the source _and_ sink

You **have** to upgrade **both** the source and sink to `v2`, or none at all,
you cannot update one without updating the other. Doing so will result in a loss
of events.

#### Zero-downtime deployment

If you want to do a zero-downtime upgrade to `v2`, you'll have to introduce the
new source/sink versions next to the existing versions, before removing the
existing one.

First, deploy the configuration that defines the source:

```diff
  [sources.vector]
    address = "0.0.0.0:9000"
    type = "vector"
+   version = "v1"

+ [sources.vector]
+   address = "0.0.0.0:5000"
+   type = "vector"
+   version = "v2"
```

Then, deploy the sink configuration, switching it over to the new version:

```diff
  [sinks.vector]
-   address = "127.0.1.2:9000"
+   address = "127.0.1.2:5000"
    type = "vector"
+   version = "v2"
```

Once the sink is deployed, you can do another deploy of the source, removing the
old version:

```diff
- [sources.vector]
-   address = "0.0.0.0:9000"
-   type = "vector"
-   version = "v1"
-
  [sources.vector]
    address = "0.0.0.0:5000"
    type = "vector"
    version = "v2"
```

That's it! You are now using the new transport protocol for Vector-to-Vector
communication.

[source]: https://vector.dev/docs/reference/configuration/sources/vector/
[sink]: https://vector.dev/docs/reference/configuration/sinks/vector/
[#2070]: https://github.com/timberio/vector/issues/2070
[#5124]: https://github.com/timberio/vector/issues/5124
[#6646]: https://github.com/timberio/vector/issues/6646
[#6032]: https://github.com/timberio/vector/pull/6032
