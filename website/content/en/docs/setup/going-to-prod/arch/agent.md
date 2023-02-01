---
title: Agent Architecture
description: Run Vector at your edge to democratize processing.
weight: 2
---

{{< warning >}}
If you have a complex production environment that makes deploying Vector as an agent difficult, consider starting with the [aggregator architecture](/docs/setup/going-to-prod/arch/aggregator) or combine them for the [unified architecture](/docs/setup/going-to-prod/arch/unified).
{{< /warning >}}

---

---

## Overview

This agent architecture deploys Vector as an [agent](/docs/setup/going-to-prod/architecting/#agent-role) on each node for local data collection and processing.

![Agent](/img/going-to-prod/agent.png)

Data can be collected directly by Vector, indirectly through another agent, or both simultaneously. Data processing can happen [locally](/docs/setup/going-to-prod/architecting/#local-processing) on the node or [remotely](/docs/setup/going-to-prod/architecting/#remote-processing) in an aggregator.

### When to Use This Architecture

We recommend this architecture for:

- Simple environments that do not require [high durability or high availability](/docs/setup/going-to-prod/high-availability/).
- Use cases that do not need to hold onto data for long periods, such as fast, stateless processing and streaming delivery. (i.e., merging multi-line logs or aggregating host specific metrics).
- Operators that can make node-level changes without a lot of friction.

If your use case violates these recommendations, consider the [aggregator](/docs/setup/going-to-prod/arch/aggregator/) or [unified](/docs/setup/going-to-prod/arch/unified/) architectures.

## Going to Production

### Architecting

- Only [replace](/docs/setup/going-to-prod/architecting/) agents that perform [generic data forwarding functions](/docs/setup/going-to-prod/architecting/#when-vector-should-replace-agents); [integrate](/docs/setup/going-to-prod/architecting/#when-vector-should-not-replace-agents) with all other agents.
- [Limit processing to fast, stateless processing](/docs/setup/going-to-prod/architecting/#local-processing). If you need complex processing, consider the [aggregator architecture](/docs/setup/going-to-prod/arch/aggregator/).
- [Limit delivery to fast, streaming delivery](/docs/setup/going-to-prod/architecting/). If you need long-lived batching consider the [aggregator architecture](/docs/setup/going-to-prod/arch/aggregator/).
- Buffer your data in memory; do not buffer on disk. If you need durability, consider the [aggregator architecture](/docs/setup/going-to-prod/arch/aggregator/).

{{< info >}}
See the [architecting document](/docs/setup/going-to-prod/architecting/) for more detail.
{{< /info >}}

### High Availability

- If the [failure of a single Vector agent](/docs/setup/going-to-prod/high-availability/#vector-process-failure) is unacceptable, consider the [aggregator architecture](/docs/setup/going-to-prod/arch/aggregator/), which deploys Vector across multiple nodes in a [highly available manner](/docs/setup/going-to-prod/high-availability/).
- Enable [end-to-end acknowledgements](/docs/about/under-the-hood/architecture/end-to-end-acknowledgements/) to mitigate [data receive failures](/docs/setup/going-to-prod/high-availability/#data-receive-failure).
- Route dropped events to mitigate [data processing failures](/docs/setup/going-to-prod/high-availability/#data-processing-failure).

{{< info >}}
See the [high availability document](/docs/setup/going-to-prod/high-availability/) for more detail.
{{< /info >}}

### Hardening

- [Secure Vector’s data](/docs/setup/going-to-prod/hardening/#securing-the-data).
- [Secure the Vector process](/docs/setup/going-to-prod/hardening/#securing-the-vector-process).
- [Secure the host](/docs/setup/going-to-prod/hardening/#securing-the-host).
- [Secure the network](/docs/setup/going-to-prod/hardening/#securing-the-network).

{{< info >}}
See the [hardening recommendations](/docs/setup/going-to-prod/hardening/) for more detail.
{{< /info >}}

### Sizing, Scaling, & Capacity Planning

- Limit the Vector agent to [2 vCPUs](/docs/setup/going-to-prod/sizing/#cpus) and [4 GiB of memory](/docs/setup/going-to-prod/sizing/#memory). If your Vector agent requires more than this, shift resource-intensive processing to your [aggregators](/docs/setup/going-to-prod/arch/aggregator/).

{{< info >}}
See the [sizing, scaling, and capacity planning document](/docs/setup/going-to-prod/sizing/) for more detail.
{{< /info >}}

### Rolling Out

- Roll out [one network partition and one system at a time](/docs/setup/going-to-prod/rollout/#incremental-adoption).
- Following the roll-out [strategy](/docs/setup/going-to-prod/rollout/#rollout-strategy) and [plan](/docs/setup/going-to-prod/rollout/#rollout-plan).

{{< info >}}
See the [rolling out document](/docs/setup/going-to-prod/rollout/) for more detail.
{{< /info >}}

## Advanced

### Working with Other Agents

We [recommend](/docs/setup/going-to-prod/architecting/) deploying Vector alongside other agents that [integrate](/docs/setup/going-to-prod/architecting/#when-vector-should-not-replace-agents) with specific systems and produce unique data. Otherwise, Vector should [replace](/docs/setup/going-to-prod/architecting/#when-vector-should-replace-agents) the agent. See the [collecting data](/docs/setup/going-to-prod/architecting/#collecting-data) section for more detail.

### Processing at the Edge

As a general rule of thumb, agents should not hold onto data. Furthermore, processing and delivery of data should be fast and streaming. If you need to perform complex processing or long-lived batching, use the [aggregator architecture](/docs/setup/going-to-prod/arch/aggregator/).

## Support

For easy setup and maintenance of this architecture, consider the Vector’s [discussions](https://discussions.vector.dev) or [chat](https://chat.vector.dev). These are free best effort channels. For enterprise needs, consider Datadog Observability Pipelines, which comes with enterprise-level support.
