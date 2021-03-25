---
title: Concepts
description: The fundamental Vector concepts. A great place to start learning about Vector.
---

<SVG src="/optimized_svg/concepts_687_357.svg" />

In order to understand Vector, you must first understand the fundamental
concepts. The following concepts are ordered progressively, starting with the
individual unit of data ([events](#events)) and broadening all the way to
Vector's deployment models ([pipelines](#pipelines)).

## Events

"Events" represent the individual units of data in Vector. They must fit into
one of the following types.

<Jump to="/docs/about/under-the-hood/architecture/data-model/">Data model</Jump>

### Logs

A "log" event is a generic key/value representation of an event.

<Jump to="/docs/about/under-the-hood/architecture/data-model/log/">Log events</Jump>

### Metrics

A "metric" event is a first-class representation of numerical operation
performed on a time series. Vector's metric events are fully interoperable.

<Jump to="/docs/about/under-the-hood/architecture/data-model/metric/">Metric events</Jump>

## Components

"Component" is the generic term we use for [sources](#sources),
[transforms](#transforms), and [sinks](#sinks). Components ingest, transform,
and route events. You compose components to create [topologies](#topology).

<Jump to="/components/">Components</Jump>

### Sources

Vector wouldn't be very useful if it couldn't ingest data. A "source" defines
where Vector should pull data from, or how it should receive data pushed to it.
A [topology](#topology) can have any number of sources, and as they ingest data
they proceed to normalize it into [events](#events) \(see next section\). This sets the stage
for easy and consistent processing of your data. Examples of sources include
[`file`][docs.sources.file], [`syslog`][docs.sources.syslog],
[`StatsD`][docs.sources.statsd], and [`stdin`][docs.sources.stdin].

<Jump to="/docs/reference/configuration/sources/">Sources</Jump>

### Transforms

A "transform" is responsible for mutating events as they are transported by
Vector. This might involve parsing, filtering, sampling, or aggregating. You can
have any number of transforms in your pipeline and how they are composed is up
to you.

<Jump to="/docs/reference/configuration/transforms/">Transforms</Jump>

### Sinks

A "sink" is a destination for [events][docs.data-model]. Each sink's
design and transmission method is dictated by the downstream service it is
interacting with. For example, the [`socket` sink][docs.sinks.socket] will
stream individual events, while the [`aws_s3` sink][docs.sinks.aws_s3] will
buffer and flush data.

<Jump to="/docs/reference/configuration/sinks/">Sinks</Jump>

## Pipeline

A "Pipeline" is a directed acyclic graph of [components](#components). Each
component is a node on the graph with directed edges. Data must flow in one
direction, from sources to sinks. Components can produce zero or more events.

<Jump to="/docs/about/under-the-hood/architecture/pipeline-model/">Pipeline model</Jump>

## Roles

A "role" refers to a deployment role that Vector fills in order to create
end-to-end pipelines.

<Jump to="/docs/setup/deployment/roles/">Deployment roles</Jump>

### Agent

The "agent" role is designed for deploying Vector to the edge, typically for
data collection.

<Jump to="/docs/setup/deployment/roles/#agent">Agent role</Jump>

### Aggregator

The "aggregator" role is designed to collect and process data from multiple
upstream sources. These upstream sources could be other Vector agents or
non-Vector agents such as Syslog-ng.

<Jump to="/docs/setup/deployment/roles/#aggregator">Aggregator role</Jump>

## Topology

A "topology" refers to the end result of deploying Vector into your
infrastructure. A topology may be as simple as deploying
Vector as an agent, or it may be as complex as deploying Vector as an agent
and routing data through multiple Vector aggregators.

<Jump to="/docs/setup/deployment/topologies/">Deployment topologies</Jump>

[docs.data-model]: /docs/about/under-the-hood/architecture/data-model/
[docs.sinks.aws_s3]: /docs/reference/configuration/sinks/aws_s3/
[docs.sinks.socket]: /docs/reference/configuration/sinks/socket/
[docs.sources.file]: /docs/reference/configuration/sources/file/
[docs.sources.statsd]: /docs/reference/configuration/sources/statsd/
[docs.sources.stdin]: /docs/reference/configuration/sources/stdin/
[docs.sources.syslog]: /docs/reference/configuration/sources/syslog/
