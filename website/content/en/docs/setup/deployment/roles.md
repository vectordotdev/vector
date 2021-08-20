---
title: Deployment roles
short: Roles
weight: 1
show_toc: true
aliases: ["/docs/setup/deployment/strategies"]
---

Vector is an end-to-end data pipeline designed to collect, process, and route data. This means that Vector serves all roles in building your pipeline. You can deploy it as an [agent](#agent), [sidecar](#sidecar), or [aggregator](#aggregator). You combine these roles to form [topologies]. In this section, we'll cover each role in detail and help you understand when to use each.

{{< warning title="Aggregator role in private beta" >}}
Helm support for the [aggregator] role is currently in private beta. We're currently seeking beta testers. If interested, please [join our chat][chat] and let us know.

As an alternative, you can still manually deploy Vector in the aggregator role. Instructions throughout this doc will be for the [agent] role only.

[agent]: /docs/setup/deployment/roles/#agent
[aggregator]: /docs/setup/deployment/roles/#aggregator
[chat]: https://chat.vector.dev
{{< /warning >}}

{{< roles >}}

The Aggregator is available in Helm. For more information about getting started with the Aggregator, see the [Helm install docs][helm].

[topologies]: /docs/setup/deployment/topologies
[helm]: /docs/setup/installation/package-managers/helm/
