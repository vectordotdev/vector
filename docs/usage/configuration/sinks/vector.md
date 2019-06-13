---
description: Forward log and metric events to another downstream Vector instance
---

# vector sink

![](../../../.gitbook/assets/vector-sink.svg)

The `vector` sink streams [`log`](../../../about/data-model.md#log) and [`metric`](../../../about/data-model.md#metric) events to another downstream Vector [service](../../../setup/deployment/roles/service.md).

## Input

The `vector` sink accepts both [`log`](../../../about/data-model.md#log) and [`metric`](../../../about/data-model.md#metric) events from a [source](../sources/) or [transform](../transforms/).

