---
date: "2021-12-17"
title: "Introducing log schema support"
description: "New log schema for end-to-end type safety"
authors: ["barieom"]
pr_numbers: [10261, 10135]
release: "0.19.0"
hide_on_release_notes: false
badges:
  type: enhancement
---

We're thrilled to announce Vector's initial support for log schema, providing type safety to
your pipeline end-to-end for `datadog_agent` source and `datadog_logs` sink.

Without log schema support, Vector users previously lacked interoperability from a log
integrations perspective. To integrate two systems, for example a `splunk_hec` source to
a `datadog_logs` sink, Vector users had to manually intervene, requiring data mapping or
transformations, as Vector could not map fields across a matrix of sources and sinks. Outside
the overhead of manual intervention, this added a risk of fields being misplaced or duplicated
in a downstream service, an issue that would only get discovered later in the development
process.

In addition, the lack of schema support caused considerable friction from a developer experience
perspective. To expand, Vector users took advantage of VRL's infallibility by ensuring type safety
through repeated calls to [coerce functions][coerce functions], such as `to_string`. However, each
call to a coercion function necessitates error handling, which can be quite a burden.

Most importantly, Vector users were aloof to malformed data sent by an upstream client sending
data to Vector. When a log with malformed schema enters a given Vector pipeline and causes an error,
admin of the pipeline would need to intervene to find that error, as Vector was unable to provide
validations errors on the edge.

With this new release, Vector's log schema support will improve your developer experience and adding
real-world reliability by guaranteeing end-to-end type safety. In short, any arbitrary log data that
gets sent to a Vector pipeline will need to match a given schema. In this context, we define log
schemas as the internal knowledge of data types — i.e. metadata — that informs features that require
type information. This initial log schema support is limited to `datadog_agent` source and
`datadog_logs` sink.

This works by Vector implying a schema of some kind from the log source, then enforcing a schema
based on the requirements of the sink. On the source side, there are two ways that Vector will imply
a log schema: sources with underlying protocols, such as `datadog_agent` and `syslog`, with known
fields or generic sources derived from configured codecs, such as `http` and `socket`, with unknown
fields. On the sinks side, Vector will enforce requirements of schemas (e.g., a sink that requires a
specific attribute)  at boot time. For example, if a Vector user is routing to a `datadog_logs` sink,
which requires specific [reserved attributes][DD reserved attributes], they will be required to
specify where those reserved attributes live and ensure they are the expected data types.

An added advantage to Vector's log schema support is that it is polymorphic, meaning that it is
flexible to support a full spectrum of strictness and backward compatible. With this release, if
your pipeline is using a source in a way that does not describe a schema, Vector will now catch and
surface mapping errors due to data type requirements.

Let's take a look at a couple examples. To start, below is a configuration for a `datadog_agent` with
a `syslog` codec getting sent to a `datadog_logs` sink:


``` toml
[sources.datadog_agent]
type = "datadog_agent"
decoding.codec = "syslog"

[sinks.datadog_logs]
type = "datadog_logs"
inputs = ["datadog_agent"]
```

In this case, data is received by the `datadog_agent` source and decoded into the [Datadog schema][DD schema]:

```json
{
  "message": "string", // semantic message
  "status": "int", // semantic severity
  "timestamp": "timestamp", // semantic timestamp
  "hostname": "string", // semantic host
  "service": "string", // semantic service
  "source": "string", // semantic source
  "tags": "string" // semantic tags
}
```

The result of the decoding is then placed in the `syslog` namespace as defined by the Datadog schema. As you
can see, the semantic message field (i.e. `message`), is decoded according to the `decoding.codec` option:

``` json
{
  "message": "string", // semantic message
  "status": "int", // semantic severity
  "timestamp": "timestamp", // semantic timestamp
  "hostname": "string", // semantic host
  "service": "string", // semantic service
  "source": "string", // semantic source
  "tags": "string",// semantic tags
  "syslog": {
    "hostname": "string",
    "appname": "string",
    "severity": "int",
    "timestamp": "string",
    "env": "string"
  }
}
```

To note, data is automatically mapped in the `datadog_logs` sink since we have semantic meaning for all
required fields.

Now, let's expand on this example and take a look at the configuration for a pipeline handling logs from
a `datadog_agent` source with a `json` codec, which gets `remapped`, then routed to `datadog_logs` sink:

``` toml
[sources.datadog_agent]
type = "datadog_agent"
decoding.codec = "json"

[transforms.remap]
type = "remap"
inputs = ["datadog_agent"]
source = ‘’’
.duration = with_semantic_meaning("duration", del(.attribute.duration)) // coerces and adds semantic context in one shot
‘’’

[sinks.datadog_logs]
type = "datadog_logs"
inputs = ["remap"]
```

Identical to the previous example, data is received by the `datadog_agent` source and decoded into the
Datadog schema:

``` json
{
  "message": "string", // semantic message
  "status": "int", // semantic severity
  "timestamp": "timestamp", // semantic timestamp
  "hostname": "string", // semantic host
  "service": "string", // semantic service
  "source": "string", // semantic source
  "tags": "string" // semantic tags
}
```

...which, again identical to the previous example, is decoded according to the `decoding.codec` option:

``` json
{
  "message": "string", // semantic message
  "status": "int", // semantic severity
  "timestamp": "timestamp", // semantic timestamp
  "hostname": "string", // semantic host
  "service": "string", // semantic service
  "source": "string", // semantic source
  "tags": "string", // semantic tags
  "attributes": {
    "string": "any - timestamp", // JSON types only
  }
}
```

Afterwards, data is processed using a `remap` transform, where an `attributes` value (i.e.
`attributes.duration`) is used as the semantic duration field. Using `with_semantic_meaning` function, Vector
coerces the value into the proper type and adds semantic meaning to the field:

``` json
{
  "message": "string", // semantic message
  "status": "int", // semantic severity
  "timestamp": "timestamp", // semantic timestamp
  "hostname": "string", // semantic host
  "service": "string", // semantic service
  "source": "string", // semantic source
  "tags": "string", // semantic tags
  "duration": "int", // semantic duration
  "attributes": {
    "string": "any - timestamp", // JSON types only
  }
}
```

For our next steps, we'll be looking to add more sources, sinks, schema types (such as support custom schemas,
ECS, and OT). In the meantime, if you any feedback for us, let us know on our [Discord chat] or on [Twitter]!


[coerce functions]: https://vector.dev/docs/reference/vrl/functions/#coerce-functions
[DD reserved attributes]: https://docs.datadoghq.com/logs/log_configuration/attributes_naming_convention/#reserved-attributes
[DD schema]: https://github.com/DataDog/agent-payload/blob/a51aceeccbf12a1f44e30b46ffcb6cbd9ce0854a/proto/logs/agent_logs_payload.proto#L11
[Discord chat]: https://discord.com/invite/dX3bdkF
[Twitter]: https://twitter.com/vectordotdev
