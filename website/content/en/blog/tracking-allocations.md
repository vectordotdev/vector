---
title: Monitoring Vector's per-component memory usage
short: Allocation tracking
description: Gaining insight into Vector's per-component memory usage.
authors: ["arshiyasolei", "tobz"]
date: "2022-12-13"
badges:
  type: announcement
  domains: ["monitoring"]
tags: ["allocation-tracing", "tracking allocations"]
---

We are excited to announce that Vector now has support for exposing per-component memory usage metrics. This work begins to address an often-requested  feature from users who want to understand how Vector uses memory, and what parts of their configuration are responsible for high memory usage.

## Trying it out

To quickly try it out, you can pass `--allocation-tracing` when launching Vector which will enable the allocation tracing feature. This will emit new memory usage metrics as part of Vector's `internal_metrics` source output, as well as expose a new column in the `vector top` user interface to show the memory usage per component.

## What's new

We provide the following new metrics: `component_allocated_bytes`, `component_allocated_bytes_total`, and `component_deallocated_bytes_total`.

- `component_allocated_bytes` captures the current net allocations/deallocations.
- `component_allocated_bytes_total` shows the accumulated total allocations.
- `component_deallocated_bytes_total` shows the accumulated total deallocations.

## How it works

Under the hood, Vector uses a custom memory allocator implementation which captures each time an allocation is made (or freed) and associates it with the currently-executing component. This works build upon some of the existing tracing functionality that Vector uses internal to provide structured logging, but has undergone a lot of work and effort around trying to optimize it for production usage.

As well, we also track allocations for the "root" component. The root component includes anything Vector allocates itself, regardless of whatever is specified in your configuration.

## Using allocation tracing in production

Currently, this feature is only supported on Linux builds of Vector. This may change in the future as we continue to improve allocation tracing.

Additionally, there is a small performance overhead imposed by allocation tracing when enabled. Vector runs a set of soak tests in CI in order to catch performance regressions during development. These soak tests emulate common Vector use cases, such as remapping events, or converting payloads from one event type to another, and so on.

In our development and testing of this feature, we've observed around a 20% reduction in throughput. This should be low enough to allow enabling it on a single Vector instance for debugging purposes, but care should be taken before enabling it across your entire fleet.

## Next steps

We currently do not provide support for determining shared memory usage between components. Adding support for shared ownership tracking provides further insights into the lifetimes of components, further easing the debugging process.

[vector]: /
[tracing]: https://docs.rs/tracing/latest/tracing/
