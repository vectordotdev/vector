---
date: "2021-08-24"
title: "Moving the Aggregator chart to public beta"
description: "The Vector Aggregator Helm chart is now publicly available"
authors: ["spencergilbert"]
pr_numbers: [TODO]
release: "0.16.0"
hide_on_release_notes: false
badges:
  type: "announcement"
  platforms: ["helm"]
---

We are happy to announce our *vector-aggregator* chart is now publicly available. While
we continue to iterate on and make improvements to the chart, we feel like it's in a good
place to get wider feedback from the community.

We have also created a dedicated Discord channel, #aggregator, for support and questions.

```shell
helm repo add vector https://helm.vector.dev
helm repo update
helm show values vector/vector-aggregator
```

The chart will also be available from https://packages.timber.io/helm/latest
for backward compatibility.

An installation with default values today will start a *vector-aggregator* listening on
port `9000` with a [v2][highlight.v2] [`vector` source][sources.vector].

[sources.vector]: /docs/reference/configuration/sources/vector/
[highlight.v2]: /highlights/2021-07-21-0-16-upgrade-guide/#vector_source_sink
