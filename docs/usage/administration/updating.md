---
description: Updating Vector to a later version
---

# Updating

This document covers how to properly update Vector.

## Quick Start

1. Start with the most downstream Vector instance. \(this depends on your [topology](../../setup/deployment/topologies.md)\)
2. [Stop Vector.](stopping.md)
3. Update Vector for your [platform](../../setup/installation/platforms/) or [operating system](../../setup/installation/operating-systems/). This should be as simple as replacing the `vector` binary.
4. [Start Vector.](starting.md)
5. Repeat with the next closest upstream Vector instance.

## Best Practices

### Working Upstream

![Where To Start Example](../../assets/updating-upstream.svg)

Depending on your [topology](../../setup/deployment/topologies.md), you'll want update your Vector instances in a specific order. You should _always_ start downstream and work your way upstream. This allows for incremental updating across your topology, ensuring downstream Vector instances do not receive data formats that are unrecognized. Vector always makes a best effort to successfully process data, but there is no guarantee of this if a Vector instance is handling a data format defined by a future unknown version.

### Capacity Planning

Because you'll be taking Vector instances offline for a short period of time, upstream data will accumulate and buffer. To avoid overloading your instances, you'll want to make sure you have enough capacity to handle the surplus of data. We recommend provisioning at least 20% of head room, on all resources, to account for spikes and updating.

## How It Works

### Back Pressure & Buffering

Vector is designed for resiliency, handling downstream disruption by [buffering](../configuration/sinks/buffer.md) data, applying [back pressure](../configuration/sinks/buffer.md#back-pressure-vs-load-shedding), and retrying until the downstream service is available again. This makes it possible to seamlessly update Vector. A downstream service can come offline and online again without losing upstream data.

### On-Disk Buffers

If you've configured your Vector instance to use on-disk buffers rest assured that Vector's buffers are designed to be backwards compatible. New Vector versions will understand old buffer formats.



