---
title: Deployment roles
short: Roles
weight: 1
show_toc: true
aliases: ["/docs/setup/deployment/strategies"]
---

Vector is an end-to-end data pipeline designed to collect, process, and route data. This means that Vector serves all roles in building your pipeline. You can deploy it as an [agent](#agent), [sidecar](#sidecar), or [aggregator](#aggregator). You combine these roles to form [topologies]. In this section, we'll cover each role in detail and help you understand when to use each.

{{< roles >}}

You can install the Vector as an Aggregator on Kubernetes using Helm. For more information about getting started with the Aggregator role, see the [Helm install docs][helm].

[topologies]: /docs/setup/deployment/topologies
[helm]: /docs/setup/installation/package-managers/helm/
