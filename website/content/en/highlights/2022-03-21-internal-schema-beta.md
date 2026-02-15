---
date: "2022-03-21"
title: "Beta release of internal schema support"
description: "increasing correctness and ergonomics"
authors: ["JeanMertz", "barieom"]
pr_numbers: [11300]
release: "0.21.0"
hide_on_release_notes: false
badges:
  type: "new feature"
  domains: ["schemas", "vrl"]
  sources: ["datadog_agent"]
  transforms: ["remap"]
  sinks: ["datadog_logs"]
---


We're thrilled to announce Vector's initial support for log schemas, which provides
end-to-end type safety in your pipelines using the `datadog_agent` source and
`datadog_logs` sink; this early beta release feature will increase your runtime
correctness and improve VRL ergonomics.
type safety to your pipeline end-to-end for `datadog_agent` source and
`datadog_logs` sink; this early beta release feature will increase your runtime
correctness and improves VRL ergonomics.

Without log schema support, Vector users previously lacked interoperability from
a log integrations perspective. To integrate two systems, for example a
`splunk_hec` source to a `datadog_logs` sink, Vector users had to manually
intervene, requiring data mapping or transformations, as Vector could not map
fields across a matrix of sources and sinks. Outside the overhead of manual
intervention, this added a risk of fields being misplaced or duplicated in a
downstream service, an issue that would only get discovered later in the
development process.

In addition, the lack of schema support caused considerable friction from a
developer experience perspective. To expand, Vector users took advantage of VRL's
infallibility by ensuring type safety through repeated calls to [coerce functions][coerce functions],
such as `to_string`. However, each call to a coercion function necessitates
error handling, which can be quite a burden.

Most importantly, Vector users did not have visibility into whether malformed
data is sent by an upstream client sending data to Vector. When a log with
malformed schema enters a given Vector pipeline and causes an error, admin
of the pipeline would need to intervene to find that error, as Vector was
unable to provide validations errors on the edge.

With this new release, Vector's log schema support will improve your developer
experience and adding real-world reliability by guaranteeing end-to-end type
safety. In short, any arbitrary log data that gets sent to a Vector pipeline will
need to match a given schema. In this context, we define log schemas as the
internal knowledge of data types — i.e. metadata — that informs features that
require type information. This initial log schema support is limited to
`datadog_agent` source and `datadog_logs` sink, but will be expanded
out to additional sources and sinks soon.

This works by Vector implying a schema of some kind from the log source, then
enforcing a schema based on the requirements of the sink. On the source side,
there are two ways that Vector will imply a log schema: sources with underlying
protocols, such as `datadog_agent` and `syslog`, with known fields or generic
sources derived from configured codecs, such as `http` and `socket`, with
unknown fields. On the sinks side, Vector will enforce requirements of schemas
(e.g., a sink that requires a specific attribute) at boot time. For example, if
a Vector user is routing to a `datadog_logs` sink, which requires specific
[reserved attributes][DD reserved attributes], they will be required to specify where
those reserved attributes live and ensure they are the expected data types.

An added advantage to Vector's log schema support is that it is polymorphic,
meaning that it is flexible to support a full spectrum of strictness and
backwards compatibility
in a way that does not describe a schema, Vector will now catch and surface
mapping errors due to data type requirements.

Read on to understand how to opt in to this feature and how you can start using
this today.

## Opting in

You can enable schema suppory by setting the top-level `schema.enabled`
configuration property:

```toml
[schema]
enabled = true
```

## How it works

Once schema support is enabled, three features are activated:

1. Schema Definitions
2. Schema Requirements
3. Semantic Fields

Each feature is explained step-by-step below, starting with schema definitions:

### Schema Definitions

A _schema definition_ is an internally typed representation of events emitted by
sources or transforms. In this initial release, we’ve added minimal support for
the `datadog_agent` source. This means that once enabled, Vector knows the
values types for the following fields originating from this source:

| Event Field  | Type(s)   | Semantic Field |
| -------------| --------- | -------------- |
| `message`    | string    | `message`      |
| `status`     | string    | `severity`     |
| `timestamp`  | timestamp | `timestamp`    |
| `hostname`   | string    | `host`         |
| `service`    | string    |                |
| `ddsource`   | string    |                |
| `ddtags`     | string    |                |

In this case, all fields are non-optional (e.g. they will always be present when
an event is emitted to the rest of the Vector topology by this source).

On its own, this definition makes any transform that uses [Vector Remap
Language][vrl] (e.g. `remap`, `route`, etc) more ergonomic to use. Vector passes
the type information to the VRL compiler, allowing VRL to type-check fields
without the need to manually define field types in VRL programs.

In practice, this means configurations such as the following:

```toml
[sources.datadog_agent]
type = "datadog_agent"
address = "0.0.0.0:80"

[transforms.remap]
type = "remap"
inputs = ["datadog_agent"]
source = '''
  .message = downcase(.message) ?? "message could be something other than a string"

  if (ms, err = to_unix_timestamp(.timestamp, unit: "milliseconds"); err == null) {
    .timestamp_ms = ms
  }
'''
```

can be simplified as such:

```toml
[schema]
enabled = true

[sources.datadog_agent]
type = "datadog_agent"
address = "0.0.0.0:80"

[transforms.remap]
type = "remap"
inputs = ["datadog_agent"]
source = '''
  .message = downcase(.message)
  .timestamp_ms = to_unix_timestamp(.timestamp, unit: "milliseconds")
'''
```
### Schema Requirements

The second part of this feature is the concept of schema requirements.

Sinks (in this intial beta, only the `datadog_logs` sink) define a set of schema
requirements to which _all_ events fed into the sink must adhere to.

Similar to schema definitions, a schema requirement can define the type and
optionality of an event field.

In the case of the `datadog_logs` sink, this is defined as follows:

| Semantic Field | Required | Type(s)   |
| -------------- | -------- | ----------|
| `message`      | ✔        | string    |
| `timestamp`    | ✔        | timestamp |
| `host`         | ✘        | string    |
| `source`       | ✘        | string    |
| `severity`     | ✘        | string    |
| `trace_id`     | ✘        | string    |

If any of the events fed to this sink by the configured topology does not match
the required schema (both optionality and types), Vector fails to boot and
a correction has to be made in the topology configuration.

It is important to note that validation happens at **boot time**, meaning once
Vector runs, the validated sink knows how to unambiguously serialize any events
it receives at runtime.

### Semantic Fields

It’s worth noting that the above table lists _semantic_ fields, whereas the
schema definition lists both _event_ fields and semantic fields. This is the
third and final part of the internal schema feature.

When a sink has an internal schema requirement defined, it defines those
requirements on _semantic fields_ in an event. A semantic field can be thought
of as a "pointer" to an actual existing field in an event.

For example, given event `{ "msg": "hello world" }`, the field `msg` can be
assigned the semantic meaning of `message`. For a second event `{ "@message":
"hello universe" }`, that same meaning can be assigned to the `@message` field.

This allows sinks to accept differently formatted events, while still knowing
how to serialize both events in accordance to the upstream API requirements.

In the case of the `datadog_logs` sink, it expect the semantic field `message`
to be present, and be a string. Both of the above events adhere to this
requirement, and thus would be accepted by the sink. At runtime, the sink will
pick the value from the correct event field (either `msg` or `@message`), before
serializing the data and sending it off to Datadog Logs. 

Knowing the above, we can also deduce that configuring a topology starting at
a `datadog_agent` source, and ending at a `datadog_logs` sink requires zero
manual configuration by the operator, because all events originating from the
source have the correct types and semantic fields configured for the sink to
serialize the data.

If you want to feed data from a non-conforming source to the `datadog_logs`
sink, you’ll need to manually map the required semantic fields, using the new
`set_semantic_meaning` function in VRL:

```coffee
set_semantic_meaning(.@message, "message")
```

You’ll also need to set the correct type for the given field, as only the
`datadog_agent` source currently supports defining type information:

```coffee
# Set `.@message` to a string if it is, or encode whatever
# type we get to a JSON encoded string.
.@message = string(.@message) ?? encode_json(.@message)
```

## Going Forward

In the coming months, we’ll be adding schema definitions to all our existing
sources and schema requirements to sinks. We’re also in the process of
evaluating the possibility of exposing custom schemas to operators.

We’re releasing this initial beta for you to experiment with; we'll be
making tweaks to improve ergonomics and reduce any performance impact
enabling schema support might have.

## Let us know what you think!

We are excited about the extra layer of ergonomics and correctness this initial
beta release of schema support brings to Vector. If you have any feedback for us
let us know on [Discord] or on [Twitter].

[vrl]: https://vrl.dev
[Discord]: https://discord.com/invite/dX3bdkF
[Twitter]: https://twitter.com/vectordotdev
