---
title: Runtime Model
description: Vector's runtime model and how it manages concurrency.
---

<SVG src="/optimized_svg/runtime-model_770_325.svg" />

Vector's runtime is a futures-based asynchronous runtime where nodes in Vector's
[DAG topology model][docs.architecture.pipeline-model] roughly map to asynchonous
[tasks](#tasks) that [communicate](#data-model) via channels, all
[scheduled](#scheduled) by the [Tokio][urls.rust_tokio] runtime.

## Tasks

Nodes in Vector's [topology][docs.architecture.pipeline-model] roughly map to
asynchronous tasks, with the exception being stateless transforms that are
inlined into the source for [concurrency][docs.architecture.concurrency-model]
reasons.

### Source tasks

[Sources][docs.sources] are tasks with an output channel. This
interface is intentionally simple and favors internal composability to allow for
maximum flexibility across Vector's wide array of sources.

### Transform tasks

[Transforms][docs.transforms] can both be tasks or stateless functions
depending on their purpose.

#### Stateless function transforms

Stateless function transforms are single operation transforms that do not
maintain state across multiple events. For example, the
[`remap` transform][docs.transforms.remap] performs individual
operations on events as they are received and immediately returns. This
function-like simplificity allows them to be inlined at the source level to
achieve our [conccurency model](concurrency-model).

#### Task transforms

Task transforms can optionally maintain state across multiple events. Therefore,
they run as separate tasks and cannot be inlined at the source level for
concurrency. An example of task transform is the
[`dedupe` transform][docs.transforms.dedupe], which maintains state to
drop duplicate events.

### Sink tasks

[Sinks][docs.sinks] are tasks with an input channel. This interface is
intentionally simple and favors internal composability to allow for maximum
flexibility. Sinks share a lot of infrastructure that make them easy and
flexible to build. Such as streaming, batching, partitioning, networking,
retries, and buffers.

## Scheduler

Vectur leverages the [Tokio][urls.rust_tokio] runtime for task scheduling.

## Data plane

Nodes in Vector's [DAG topology][docs.architecture.pipeline-model] communicate
via channels. Edge nodes are customized channels with dynamic output control
where back pressure is the default, but can be customized on a per-sink basis to
shed load or persist to disk.

[docs.architecture.concurrency-model]: /docs/about/under-the-hood/architecture/concurrency-model/
[docs.architecture.pipeline-model]: /docs/about/under-the-hood/architecture/pipeline-model/
[docs.sinks]: /docs/reference/configuration/sinks/
[docs.sources]: /docs/reference/configuration/sources/
[docs.transforms.dedupe]: /docs/reference/configuration/transforms/dedupe/
[docs.transforms.remap]: /docs/reference/configuration/transforms/remap/
[docs.transforms]: /docs/reference/configuration/transforms/
[urls.rust_tokio]: https://github.com/tokio-rs/tokio
