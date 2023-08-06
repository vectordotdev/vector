---
title: Monitoring Vector's per-component memory usage
short: Allocation tracking
description: Gaining insight into Vector's per-component memory usage.
authors: ["arshiyasolei", "tobz"]
date: "2022-12-14"
badges:
  type: announcement
  domains: ["monitoring"]
tags: ["allocation-tracing", "tracking allocations"]
---

We are excited to announce that Vector now has support for exposing per-component memory usage metrics. This work addresses an often-requested feature from users who want to understand how Vector uses memory, and what parts of their configurations are responsible for high memory usage.

## Trying it out

![vector top with allocation tracing](/img/blog/vector-top-allocation-tracking.png)

To explore the feature, you'll need version [v0.27.0](https://vector.dev/download/), or later, of Vector. Once you've got that, start Vector with the `--allocation-tracing` flag, launch `vector top` and monitor your components!

## What's new

We provide the following new metrics: `component_allocated_bytes`, `component_allocated_bytes_total`, and `component_deallocated_bytes_total`. These metrics are tagged in the same way as other internal `component_*` metrics, allowing you to drill down on the memory usage of a particular component, or type of component.

- `component_allocated_bytes` shows the current net allocations/deallocations.
- `component_allocated_bytes_total` and `component_deallocated_bytes_total` show the total number of bytes allocated and deallocated, respectively.

The following image visualizes the `component_allocated_bytes` metric of a sample Vector instance:
![Visualization of the allocation tracking metrics via Datadog](/img/blog/vector-allocation-tracking-graph.png)

## How it works

Under the hood, Vector uses a custom memory allocator implementation that captures each allocation/deallocation and associates it with the currently-executing component. This work builds upon Vector's existing tracing functionality. Allocations that aren't associated with any components are tracked by a `root` component.

## Using allocation tracing in production

Currently, this feature is only supported on Unix builds of Vector. This may change in the future as we continue to improve allocation tracing.

Additionally, there is a performance overhead imposed by allocation tracing when enabled and no overhead when disabled. Vector runs a set of soak tests in CI to catch performance regressions during development. These soak tests emulate common Vector use cases, such as remapping events, or converting payloads from one event type to another, and so on.

In our development and testing of this feature, we've observed ~20% reduction in throughput. This overhead may be acceptable for debugging purposes, but care should be taken before enabling it across your entire fleet.

## Next steps

We currently do not provide support for tracking memory ownership between components. For example, when a sink is batching events, and is configured to batch many events, you may observe high Vector memory usage. If you looked at the memory usage metrics, you would see most of it being attributed to components that either created the events (such as a source) or processed the events (such as any transforms that modified the event) rather than the sink which is batching the events. Adding support for shared ownership tracking provides further insights into the lifetimes of components, further easing the debugging process.

Please Let us know your feedback and suggestions [here](https://github.com/vectordotdev/vector/issues/15474)!
