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

Vector 0.11 includes two new metrics sources:

1. The [`apache_metrics` source][apache_metrics_source]
2. The [`mongodb_metrics` source][mongodb_metrics_source]

And while these are only two sources, they represent a broader initiative
to replace metrics agents entirely. A lot of ground work was laid to expedite
these types of integrations, so you can expect many more of them in the
subsequent Vector releases.

## Agent fatigue, we're coming for you

For anyone that manages observability pipelines, it's not uncommon to deploy
multiple agents on a single host (an understatement). We've seen setups
that deploy five or more agents on a single host -- using more than _30% of the
CPU resources for that host_! We cover this in detail in our
[Kubernetes announcements post][kubernetes_announcement]. Needless to say,
it's a very real and very expensive problem. Vector has it's sights set on
solving this. It is our intent for Vector to be the single pipeline for
all of your logs, metrics, and traces.

## Get Started

Getting started with these two sources is easy as pie. Just define the sources
and go:

```toml
[sources.apache_metrics]
type = "apache_metrics"
```

And for Mongo:

```toml
[source.mongodb_metrics]
type = "mongodb_metrics"
```

Then connect it all to a sink:

```toml
[sinks.prometheus]
type = "prometheus"
inputs = ["apache_metrics", "mongodb_metrics"]
```

Tada! One agent for all of your data. Checkout the docs for more details.

[apache_metrics_source]: ...
[mongodb_metrics_source]: ...
