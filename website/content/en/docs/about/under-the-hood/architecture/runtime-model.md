---
title: Runtime Model
description: Vector's runtime model and how it manages concurrency
weight: 3
tags: ["runtime", "concurrency", "state", "scheduler"]
---

{{< svg "img/runtime-model.svg" >}}

Vector's runtime is a futures-based asynchronous runtime where nodes in Vector's [DAG topology model][pipeline] roughly map to asynchronous [tasks](#tasks) that communicate via channels, all [scheduled](#scheduler) by the [Tokio][tokio] runtime.

## Tasks

Nodes in Vector's [topology][pipeline] roughly map to asynchronous tasks, with the exception being stateless transforms that are inlined into the source for [concurrency][concurrency] reasons.

### Source tasks

[Sources][sources] are tasks with an output channel. This interface is intentionally simple and favors internal composability to allow for maximum flexibility across Vector's wide array of sources.

### Transform tasks

[Transforms][transforms] can both be tasks or stateless functions depending on their purpose.

#### Stateless function transforms

Stateless function transforms are single operation transforms that do not maintain state across multiple events. For example, the [`remap` transform][remap] performs individual operations on events as they are received and immediately returns. This function-like simplicity allows them to be inlined at the source level to achieve our [concurrency model][concurrency].

#### Task transforms

Task transforms can optionally maintain state across multiple events. Therefore, they run as separate tasks and cannot be inlined at the source level for concurrency. An example of task transform is the [`dedupe` transform][dedupe], which maintains state to drop duplicate events.

### Sink tasks

[Sinks][sinks] are tasks with an input channel. This interface is intentionally simple and favors internal composability to allow for maximum flexibility. Sinks share a lot of infrastructure that make them easy and
flexible to build. Such as streaming, batching, partitioning, networking, retries, and buffers.

## Scheduler

Vector uses the [Tokio][tokio] runtime for task scheduling.

## Data plane

Nodes in Vector's [DAG topology][pipeline] communicate via channels. Edge nodes are customized channels with dynamic output control where back pressure is the default, but can be customized on a per-sink basis to shed load or persist to disk.

[concurrency]: /docs/about/under-the-hood/architecture/concurrency-model
[dedupe]: /docs/reference/configuration/transforms/dedupe
[pipeline]: /docs/about/under-the-hood/architecture/pipeline-model
[remap]: /docs/reference/configuration/transforms/remap
[tokio]: https://tokio.rs
[sinks]: /docs/reference/configuration/sinks
[sources]: /docs/reference/configuration/sources
[transforms]: /docs/reference/configuration/transforms
