---
description: Accept log and metrics events from an upstream Vector instance
---

# vector source

![](../../../.gitbook/assets/vector-source.svg)

The `vector` source allows you to ingest both [`log`](../../../about/data-model.md#log) and [`metric`](../../../about/data-model.md#metric) events from another upstream Vector [agent](../../../setup/deployment/roles/agent.md) or [service](../../../setup/deployment/roles/service.md).

## Output

The `vector` source outputs [`log`](../../../about/data-model.md#log) and [`metrics`](../../../about/data-model.md#metric) events in their original upstream structure.

## How It Works

### Guarantees

The `tcp` source is capable of achieving an [**at least once delivery guarantee**](../../../about/guarantees.md#at-least-once-delivery) if your [pipeline is configured to achieve this](../../../about/guarantees.md#at-least-once-delivery).

