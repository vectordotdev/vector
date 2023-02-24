---
title: Aggregator Architecture
description: Deploy Vector in your clusters to receive data from all your systems.
weight: 1
---

## Overview

The aggregator architecture deploys Vector as an [aggregator](/docs/setup/going-to-prod/architecting/) onto dedicated nodes for [remote processing](/docs/setup/going-to-prod/architecting/). Data ingests from one or more upstream agents or upstream systems:

![Aggregator](/img/going-to-prod/aggregator.png)

We recommend this architecture to most Vector users for its [high availability](/docs/setup/going-to-prod/high-availability/) and easy setup.

### When to Use this Architecture

We recommend this architecture for environments that require [high durability and high availability](/docs/setup/going-to-prod/high-availability/) (most environments). This architecture is easy to set up and slot into complex environments without changing agents. It is exceptionally well suited for enterprises and large organizations.

## Going to Production

### Architecting

- Deploy multiple aggregators within each network boundary (i.e., each Cluster or VPC).
- Use DNS or service discovery to route agent traffic to your aggregators.
- Use HTTP-based protocols when possible.
- Use the `vector` source and sink for inter-Vector communication.
- Shift the responsibility of data processing and durability to your aggregators.
- Configure your agents to be simple data forwarders.

{{< info >}}
See the [architecting document](/docs/setup/going-to-prod/architecting/) for more detail.
{{< /info >}}

### High Availability

- Deploy your aggregators across multiple nodes and availability zones.
- Enable end-to-end acknowledgements for all sources.
- Use disk buffers for your system of record sink.
- Use waterfall buffers for your system of analysis sink.
- Route failed data to your system of record.

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

- [Front your aggregators with a load balancer](/docs/setup/going-to-prod/sizing/).
- Provision at least [4 vCPUs](/docs/setup/going-to-prod/sizing/#cpus) and [8 GiB of memory](/docs/setup/going-to-prod/sizing/#memory) per instance.
- Enable [autoscaling](/docs/setup/going-to-prod/sizing/#autoscaling) with a target of 85% CPU utilization.

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

### Pub-Sub Systems

{{< warning >}}
We do not recommend provisioning a new pub-sub service for the sole purpose of Vector. Vector can deploy in a [highly available manner](/docs/setup/going-to-prod/high-availability/) that minimizes the need for such systems.
{{< /warning >}}

The aggregator architecture can deploy as a consumer to a pub-sub service, like Kafka:

![Aggregator](/img/going-to-prod/pub-sub.png)

#### Partitioning

Partitioning, or “topics” in Kafka terminology, refers to separating data in your pub-sub systems. We strongly recommend partitioning along data origin lines, such as the service or host that generated the data.

![Aggregator](/img/going-to-prod/partitioning.png)

#### Recommendations

- Use memory buffers with `buffers.when_full` set to `block`. This will ensure back pressure flows upstream to your pub-sub system, where durable buffering should occur.
- Enable [end-to-end acknowledgements](/docs/about/under-the-hood/architecture/end-to-end-acknowledgements/) for your Vector pub-sub source (i.e., the `kafka` source) to ensure data is persisted downstream before removing the data from your pub-sub systems.

### Global Aggregation

Because Vector can deployed anywhere in your infrastructure, it offers a unique approach to global aggregation. Aggregation can be tiered, allowing local aggregators to reduce data before forwarding to your global aggregators.

![Aggregator](/img/going-to-prod/global-aggregation.png)

This eliminates the need to deploy a single monolith aggregator, creating an unnecessary single point of failure. Therefore, global aggregation should be limited to use cases that can reduce data, such as computing global histograms.

#### Recommendations

- Limit global aggregation to tasks that can reduce data, such as computing global histograms. Never send all data to your global aggregators.
- Continue to use your local aggregators to process and deliver most data. Never introduce a single point of failure.

## Support

For easy setup and maintenance of this architecture, consider the Vector’s [discussions](https://discussions.vector.dev) or [chat](https://chat.vector.dev). These are free best effort channels. For enterprise needs, consider Datadog Observability Pipelines, which comes with enterprise-level support. Read more about that product [here](https://www.datadoghq.com/product/observability-pipelines/).
