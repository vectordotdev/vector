---
title: Deployment roles
short: Roles
weight: 1
---

Vector is an end-to-end data pipeline designed to collect, process, and route data. This means that Vector serves all roles in building your pipeline. You can deploy it as an [agent](#agent), [sidecar](#sidecar), or [aggregator](#aggregator). You combine these roles to form [topologies]. In this section, we'll cover each role in detail and help you understand when to use each.

## Agent

### Daemon

{{< svg "img/daemon-role.svg" >}}

The daemon role is designed to collect *all* data on a single host. This is the recommended role for data collection since it the most efficient use of host resources. Vector implements a directed acyclic graph topology model, enabling the collection and processing from mutliple services.

### Sidecar

{{< svg "img/sidecar-role.svg" >}}

The sidecar role couples Vector with each service, focused on data collection for that individual service only. While the daemon role is recommended, the sidecar role is beneficial when you want to shift reponsibility of data collection to the service owner. And, in some cases, it can be simpler to manage.

## Aggregator

{{< svg "img/aggregator-role.svg" >}}

The aggregator role is designed for central processing, collecting data from multiple upstream sources and performing cross-host aggregation and analysis.

For Vector, this role should be reserved for exactly that: cross-host aggregation and analysis. Vector is unique in the fact that it can serve both as an agent and aggregator. This makes it possible to distribute processing along the edge (recommended). We highly recommend pushing processing to the edge when possible since it is more efficient and easier to manage.

[topologies]: /docs/setup/deployment/topologies
