---
date: "2021-08-24"
title: "Moving the aggregator Helm chart to public beta"
description: "The Vector aggregator Helm chart is now publicly available"
authors: ["spencergilbert"]
pr_numbers: [8801]
release: "0.16.0"
hide_on_release_notes: false
badges:
  type: "announcement"
  platforms: ["helm", "kubernetes"]
---

We're happy to announce that our [`vector-aggregator` Helm Chart][chart] is now
in public beta! This chart has undergone rigorous testing with our internal
design partners and is now ready for wider community feedback.

## Getting Started

* Follow the [Helm installation instructions][setup].
* Checkout the [Helm chart][chart] for a full list of options.
* Hop in our [#aggregator Discord channel][discord] for help.

## What is an aggregator

When Vector is deployed as an [aggregator][aggregator], it is used to transform and ship data
collected by other agents. The core benefit of having distinct agents and aggregators is that
you can have a separation of concerns within an observability data processing pipeline. Agents
can become much “thinner,” in some cases acting as pure pipes that collect from a source and
ship downstream to aggregators, while aggregators perform the “thicker” work of ensuring that
the data is scrubbed of sensitive information, properly formatted for downstream consumers,
sampled to reduce volume, and more. This is especially useful when inserting Vector into
existing setups.

The `vector-aggregator` chart deploys Vector as a [StatefulSet][statefulset], with the option of
installing a HAProxy Deployment for load balancing across multiple aggregators.

## Setup and installation

Instructions on how setup and install the `vector-aggregator` chart can be found [here][setup].

## Looking forward

Our next steps involve improving the out-of-the-box experience and reducing the configuration
required to aggregate events from a number of common sources. We are also looking to provide
a better experience integrating load balancing across multiple aggregators with additional work
on the bundled HAProxy deployment.

[aggregator]: /docs/setup/deployment/roles/#aggregator
[chart]: https://github.com/vectordotdev/helm-charts/blob/master/charts/vector-aggregator
[discord]: https://discord.gg/Ywcq9cWEPE
[setup]: /docs/setup/installation/package-managers/helm/#aggregator
[statefulset]: https://kubernetes.io/docs/concepts/workloads/controllers/statefulset/
