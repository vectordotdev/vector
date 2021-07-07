---
title: Concepts
weight: 2
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

## Components

**Component** is the generic term for [sources], [transforms], and [sinks]. Components ingest, transform, and route events. You compose components to create [topologies].

{{< jump "/components" >}}

### Sources

Vector wouldn't be very useful if it couldn't ingest data. A **source** defines where Vector should pull data from, or how it should receive data pushed to it. A [topology][topologies] can have any number of sources, and as they ingest data they proceed to normalize it into [events] (see the next section). This sets the stage for easy and consistent processing of your data. Examples of sources include `file`, `syslog`, `statsd`, and `stdin`.

{{< jump "/sources" >}}

### Transforms

A **transform** is responsible for mutating events as they are transported by Vector. This might involve parsing, filtering, sampling, or aggregating. You can have any number of transforms in your pipeline and how they are composed is up to you.

{{< jump "/transforms" >}}

### Sinks

A **sink** is a destination for events. Each sink's design and transmission method is dictated by the downstream service it interacts with. The `socket` sink, for example, streams individual events, while the `aws_s3` sink buffers and flushes data.

{{< jump "/sinks" >}}

## Pipeline

A **pipeline** is a [directed acyclic graph][dag] of [components]. Each component is a node in the graph with directed edges. Data must flow in one direction, from sources to sinks. Components can produce zero or more events.

{{< jump "/docs/about/under-the-hood/architecture/pipeline-model" >}}

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
