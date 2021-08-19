---
date: "2021-08-24"
title: "Moving the aggregator chart to public beta"
description: "The Vector aggregator Helm chart is now publicly available"
authors: ["spencergilbert"]
pr_numbers: [8801]
release: "0.16.0"
hide_on_release_notes: false
badges:
  type: "announcement"
  platforms: ["helm"]
---

We are happy to announce that our [`vector-aggregator`][chart] chart is now publicly available.
While we continue to iterate on and make improvements to the chart, we feel it is ready to get
wider feedback from the community at large.

We have also created a dedicated Discord channel, [#aggregator][discord], for support, questions,
and feedback.

## What is an aggregator

When Vector is run in an [aggregator role][aggregator], it is used to transform and ship data
collected by other agents.

The `vector-aggregator` chart deploys Vector as a StatefulSet, with the option of installing a
HAProxy Deployment for load balancing across multiple aggregators.

## Setup and installation

Instructions on how setup and install the `vector-aggregator` chart can be found [here][setup].

## Looking forward

Our next steps involve improving the out-of-the-box experience and reducing the configuration
required to aggregate events from a number of common sources. We are also looking to provide
a better experience integrating load balancing across multiple aggregators with additional work
on the bundled HAProxy deployment.

[aggregator]: /docs/setup/deployment/roles/#aggregator
[chart]: https://github.com/timberio/helm-charts/blob/master/charts/vector-aggregator
[discord]: https://discord.gg/Ywcq9cWEPE
[setup]: /docs/setup/installation/package-managers/helm/#aggregator
