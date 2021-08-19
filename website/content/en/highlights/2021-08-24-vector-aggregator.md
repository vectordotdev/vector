---
date: "2021-08-24"
title: "Moving the Aggregator chart to public beta"
description: "The Vector Aggregator Helm chart is now publicly available"
authors: ["spencergilbert"]
pr_numbers: [8801]
release: "0.16.0"
hide_on_release_notes: false
badges:
  type: "announcement"
  platforms: ["helm"]
---

We are happy to announce that our `vector-aggregator` chart is now publicly available. While
we continue to iterate on and make improvements to the chart, we feel it is ready to get
wider feedback from the community at large.

We have also created a dedicated Discord channel, [#aggregator][discord], for support and questions.

## Setup

```shell
helm repo add vector https://helm.vector.dev
helm repo update
```

The chart will also be available from https://packages.timber.io/helm/latest
for backward compatibility.

## Installation

```shell
helm install vector vector/vector-aggregator \
  --namespace vector \
  --create-namespace
```

An installation with default values today will start a `vector-aggregator` listening on
port `9000` with a [`vector` source][sources.vector] configured with [v2][highlight.v2].

You can review the default values at [timberio/helm-charts][default] or by running the following:

```shell
helm show values vector/vector-aggregator
```

## Looking Forward

Our next steps involve improving the out-of-the-box experience and reducing the configuration
required to aggregate events from a number of common sources. We are also looking to provide
a better experience integrating load balancing across multiple Aggregators with additional work
on the bundled HAProxy installation.

[discord]: https://discord.gg/Ywcq9cWEPE
[sources.vector]: /docs/reference/configuration/sources/vector/
[highlight.v2]: /highlights/2021-07-21-0-16-upgrade-guide/#vector_source_sink
[default]: https://github.com/timberio/helm-charts/blob/master/charts/vector-aggregator/values.yaml
