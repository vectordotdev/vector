---
last_modified_on: "2020-10-27"
$schema: ".schema.json"
title: "New `*_metrics` sources"
description: "A foray into collecting metrics."
author_github: "https://github.com/binarylogic"
pr_numbers: [1314]
release: "0.11.0"https://github.com/Lusitaniae/apache_exporter
hide_on_release_notes: false
tags: ["type: announcement"]
---

Vector 0.11 includes four new metrics sources:

1. The [`host_metrics` source][host_metrics_source]
2. The [`apache_metrics` source][apache_metrics_source]
3. The [`mongodb_metrics` source][mongodb_metrics_source]
4. The [`internal_metrics` source][internal_metrics_source]

And while these are only four sources, they represent a broader initiative
to replace metrics agents entirely. A lot of groundwork was laid to expedite
these types of integrations, so you can expect many more of them in
subsequent Vector releases.

## Agent fatigue, we're coming for you

For anyone that manages observability pipelines, it's not uncommon to deploy
multiple agents on a single host (an understatement). We've seen setups
that deploy five or more agents on a single host -- using more than _30% of the
CPU resources for that host_! We cover this in detail in our
[Kubernetes announcements post][kubernetes_announcement]. It's a genuine and
costly problem. Vector has its sights set on solving this. We want Vector to be
the single pipeline for all of your logs, metrics, and traces.

## Get Started

To get started with these sources, define them and go:

```toml
[sources.host_metrics]
type = "host_metrics" # or apache_metrics, mongodb_metrics, or internal_metrics
```

Then connect them to a sink:

```toml
[source.prometheus]
type = "prometheus"
inputs = ["host_metrics"]
```

Tada! One agent for all of your data. Check out the docs for more details.

[apache_metrics_source]: ...
[mongodb_metrics_source]: ...
