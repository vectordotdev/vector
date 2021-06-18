---
title: Glossary
description: Vector glossary of terms
---

The glossary contains common terms and their definitions.

## Batch

"Batch" refers to a [batched payload][log] within a sink. It is a batch of events encoded into a payload that the downstream service understands.

## Benchmark

"Benchmark" refers to a test designed to measure performance and resource usage. You can learn more about Vector's benchmarks in Vector's [main README][performance] section.

## Binary

"Binary" refers to the static binary that Vector compiles to.

## Buffer

"Buffer" refers to an ordered queue of events that is coupled with a sink.

## Configuration

"Configuration" refers to the settings and options used to control Vector's behavior. You can learn more about Vector's configuration in the [Configuration][configuration] section.

## Durability

"Durability" refers to the ability to retain data across exceptional events. In the context of Vector, this typically refers to the ability to retain data across restarts.

## Event

"Event" refers to a single unit of data that flows through Vector. You can learn more about events in the [Data Model][data_model] section.

## Filter

"Filter" refers to a type of [transform][transforms] that filters events or fields on an event.

## Flush

"flush" refers to the act of sending a batched payload to a downstream service. It is commonly used in conjunction with "buffer".

## Github

[**Github**](https://github.com/) refers to the service used to host Vector's source code.

## Guide

"Guide" is a tutorial or walkthrough of a specific subject. You can see Vector's guides in the [Guides][guides] section.

## Log

"Log" refers to an individual log event. This is a type of [Vector event][metric].

## Metric

"Metric" refers to an individual data unit used to represent a point in time measurement. This is a type of [Vector event][metric].

## Parser

"Parser" refers to a [transform][transforms] that parses event data.

## Pipeline

"Pipeline" refers to the end result of combining [sources][sources], [transforms][transforms], and [sinks][sinks].

## Reducer

"Reducer" refers to a [transform][transforms] that reduces data into
a metric.

## Repo

"Repo" refers to a Git repository, usually the [Vector Git repository][vector_repo].

## Role

"Role" refers to a [role][roles] under which Vector is deployed.

## Router

"Router" refers is something that accepts and routes data to many destinations, this is commonly used to describe Vector.

## Rust

"Rust" refers to the [Rust programming language][rust] that Vector is written in.

## Sample

"Sample" refers to a [transform][transforms] that samples data.

## Sink

"Sink" refers to the Vector [sink concept][sinks].

## Source

"Source" refers to the Vector [source concept][sources].

## Structured Log

"Structured log" refers to a log represented in a structured form, such as a map. This is different from a text log, which is represented as a single text string.

## Table

"Table" refers to the [TOML table type][toml_table].

## TOML

"TOML" refers to [Tom's Obvious Markup Language][toml] and it is the syntax used to represent the Vector configuration.

## Topology

"Topology" refers to a [deploy topology][topologies] that Vector is deployed under.

## Transform

"Transform" refers to the Vector [transform concept][transforms].

## Use Case

"Use case" refers to a way in which Vector is used, such logs, metrics, reducing cost, etc.

## Vector

"Vector" is the name of this project.

[configuration]: /docs/reference/configuration
[data_model]: /docs/about/under-the-hood/architecture/data-model
[guides]: /guides
[log]: /docs/about/under-the-hood/architecture/data-model/log
[metric]: /docs/about/under-the-hood/architecture/data-model/metric
[performance]: https://github.com/timberio/vector#performance
[roles]: /docs/setup/deployment/roles
[rust]: https://www.rust-lang.org
[sinks]: /docs/reference/configuration/sinks
[sources]: /docs/reference/configuration/sources
[toml]: https://github.com/toml-lang/toml
[toml_table]: https://github.com/toml-lang/toml#table
[topologies]: /docs/setup/deployment/topologies
[transforms]: /docs/reference/configuration/transforms
[vector_repo]: https://github.com/timberio/vector
