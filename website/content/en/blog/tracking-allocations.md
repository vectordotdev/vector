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

We are excited to announce that [Vector] now provides per-component memory usage metrics. To explore the feature, start a vector instance with the `allocation-tracing` option, launch `vector top` and monitor your components! This feature leverages a custom wrapping allocator combined with a separate reporting thread to collect our new metrics. 

## What's new

We provide the following new metrics: `component_allocated_bytes`, `component_allocated_bytes_total`, and `component_deallocated_bytes_total`.

- `component_allocated_bytes` captures the current net allocations/deallocations.
- `component_allocated_bytes_total` shows the accumulated total allocations.
- `component_deallocated_bytes_total` shows the accumulated total deallocations. 

## How it works

From a high level point of view, we leverage our current component [tracing] infrastructure to track when a component "enters"/"exits" on a given thread. During each allocation/deallocation, we determine the responsible component via state stored in thread locals. Allocations that aren't associated with any components are tracked by a `root` component.

## Notes

This feature currently only supports unix based platforms. When enabled, there is approximately 20% overhead in throughput based on our benchmarks.

## Next steps

We currently do not provide support for determining shared memory usage between components. Adding support for shared ownership tracking provides further insights into the lifetimes of components, further easing the debugging process.

[vector]: /
[tracing]: https://docs.rs/tracing/latest/tracing/
