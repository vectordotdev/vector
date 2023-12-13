---
title: Concepts
weight: 3
tags: ["concepts", "meta", "events", "logs", "metrics", "components", "sources", "transforms", "sinks", "pipeline", "roles", "agent", "aggregator", "topology"]
---

In order to understand Vector, you must first understand the fundamental concepts. The following concepts are ordered progressively, starting with the individual unit of data ([events]) and broadening all the way to Vector's deployment models ([pipelines]).

## Events

**Events** represent the individual units of data in Vector.

{{< jump "/docs/about/under-the-hood/architecture/data-model" >}}

### Logs

A **log** event is a generic key/value representation of an event.

{{< jump "/docs/about/under-the-hood/architecture/data-model/log" >}}

### Metrics

A **metric** event represents a numerical operation performed on a time series. Vector's metric events are fully interoperable.

{{< jump "/docs/about/under-the-hood/architecture/data-model/metric" >}}

### Traces

A **trace** event can be thought of as a special kind of log event. The components that support trace events are: the `datadog_agent` source, the `datadog_traces` sink, and the `sample` and `remap` transforms. **Note**: Support for traces is limited and is in alpha.

If you're interested in using traces with a Vector component that doesn't yet support them, please open an issue so we can have a better understanding of what components to prioritize adding trace support for.

## Components

**Component** is the generic term for [sources], [transforms], and [sinks]. Components ingest, transform, and route events. You compose components to create [topologies].

{{< jump "/components" >}}

### Sources

Vector wouldn't be very useful if it couldn't ingest data. A **source** defines where Vector should pull data from, or how it should receive data pushed to it. A [topology][topologies] can have any number of sources, and as they ingest data they proceed to normalize it into [events] (see the next section). This sets the stage for easy and consistent processing of your data. Examples of sources include `file`, `syslog`, `statsd`, and `stdin`.

{{< jump "/sources" >}}

### Transforms

A **transform** is responsible for mutating events as they are transported by Vector.
This might involve parsing, filtering, sampling, or aggregating.
You can have any number of transforms in your pipeline, and how they are composed is up to you.

{{< jump "/transforms" >}}

### Sinks

A **sink** is a destination for events. Each sink's design and transmission method is dictated by the downstream service it interacts with. The `socket` sink, for example, streams individual events, while the `aws_s3` sink buffers and flushes data.

{{< jump "/sinks" >}}

## Pipeline

A **pipeline** is a [directed acyclic graph][dag] of [components]. Each component is a node in the graph with directed edges. Data must flow in one direction, from sources to sinks. Components can produce zero or more events.

{{< jump "/docs/about/under-the-hood/architecture/pipeline-model" >}}


## Buffers

Sinks try to send events as fast as possible. If they are unable to keep up, they have a configurable buffer that will hold events until they can be sent.
By default, Vector uses an in-memory buffer, but a disk-buffer is also available. Once a buffer fills up, the behavior is configurable.

`buffer.when_full = block`
This is the default behavior. When a buffer fills up, backpressure will be applied to previous components in the graph.

`buffer.when_full = drop_newest`
When a buffer fills up, new events will be dropped. This does _not_ provide backpressure.

View the full configuration options for buffers [here](/docs/reference/configuration/sinks/vector/#buffer).

## Backpressure

If a sink's buffer fills up and is configured to provide backpressure, that backpressure will propagate to any connected
transforms, which will also propagate to the sources. The sources attempt to propagate backpressure to
whichever system is providing data. The exact mechanism varies with the source. For example, HTTP sources _may_
reject requests with an HTTP 429 error (Too Many Requests), or pull-based sources such as Kafka _may_ slow down fetching new events.

Since Vector allows configuring components as a directed acyclic graph, understanding how backpressure works when there
are multiple sinks or sources involved is important.

A source only sends events as fast as the _slowest_ sink that is configured to provide backpressure (`buffer.when_full = block`).

For example, if you have a single source sending to 3 sinks in this configuration, the source will start providing
backpressure from sink 2 (500 events/sec) since that is the slowest sink configured to provide backpressure.
Sink 1 will drop up to 250 events/sec, and sink 3 will be underutilized.

- Sink 1: Can send at 250 events/sec (`buffer.when_full = drop_newest`)
- Sink 2: Can send at 500 events/sec  (`buffer.when_full = block`)
- Sink 3: Can send at 1000 events/sec  (`buffer.when_full = block`)

If there are multiple sources configured for a single component, Vector currently makes no guarantees
which source will have priority during backpressure. To make sure all inputs are fully processed, make
sure the downstream components are able to handle the volume of all the connected sources.


## Roles

A **role** is a deployment role that Vector fills in order to create end-to-end pipelines.

{{< jump "/docs/setup/deployment/roles" >}}

### Agent

The [**agent**](/docs/setup/deployment/roles#agent) role is designed for deploying Vector to the edge, typically for data collection.

### Aggregator

The [**aggregator**](/docs/setup/deployment/roles#aggregator) role is designed to collect and process data from multiple upstream sources. These upstream sources could be other Vector agents or non-Vector agents such as Syslog-ng.

## Topology

A **topology** is the end result of deploying Vector into your infrastructure. A topology may be as simple as deploying Vector as an agent, or it may be as complex as deploying Vector as an agent and routing data through multiple Vector aggregators.

{{< jump "/docs/setup/deployment/topologies" >}}

[components]: /components
[dag]: https://en.wikipedia.org/wiki/Directed_acyclic_graph
[events]: #events
[pipelines]: #pipeline
[sinks]: #sinks
[sources]: #sources
[topologies]: #topology
[transforms]: #transforms
