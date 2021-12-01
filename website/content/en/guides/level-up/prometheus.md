---
title: Using Vector with Prometheus
short: Prometheus
description: Spice up your Prometheus pipelines
author_github: https://github.com/lucperkins
domain: metrics
weight: 5
tags: ["prometheus", "metrics", "level up", "guides", "guide"]
---

[Prometheus] is a popular open source metrics platform under the auspices of the [Cloud Native
Computing Foundation][cncf] (CNCF).

You can use Vector in conjunction with Prometheus in a wide variety of ways. In this guide, we'll
explore Vector's Prometheus-related [components](#components) and cover several [use
cases](#use-cases) for Vector in observability data pipelines involving Prometheus.

## Components

Vector provides several [sources](#sources) and [sinks](#sinks) that directly pertain to Prometheus,
as well as one [transform](#transforms).

### Prometheus sources {#sources}

Vector has two Promtheus-related [sources] that you can use in your observability data pipelines:

Source | What it enables
:------|:---------------
[`prometheus_remote_write`][remote_write_source] | Use Vector as a Prometheus [remote write][remote_write] endpoint
[`prometheus_scrape`][scrape_source] | Use Vector to scrape metrics directly from any [Prometheus exporter][exporter].

### Prometheus sinks {#sinks}

Vector offers two Prometheus-related [sinks] that you can use in your observability data pipelines:

Sink | What it enables
:----|:---------------
[`prometheus_remote_write`][remote_write_sink] | Ship metrics to any [Prometheus remote write][remote_write] endpoint, which includes a wide variety of [storage systems and services][remote_endpoints].
[`prometheus_exporter`][exporter_sink] | Make Vector essentially act like any standard [Prometheus exporter][exporter] that exposes Prometheus metrics via HTTP.

### Prometheus-related transforms {#transforms}

Although Vector doesn't provide any Prometheus-specific transforms, one transform that can be quite
useful when used in conjunction with Prometheus is the
[`tag_cardinality_limit`][tag_cardinality_limit] transform, which enables you to limit the number of
tags attached to metric events.

[High label cardinality][cardinality] is a common problem when using Prometheus (the terms "label"
and "tag" are synonymous in this context). High cardinality is typically caused by applications that
"misbehave"—usually unwittingly—by adding labels to metrics in a way that causes significant burden
on downstream Prometheus systems that need to index and store those metrics. With Vector's
[`tag_cardinality_limit`][tag_cardinality_limit] transform, you can [drop] either tags (labels) that
exceed your configured limits or entire metric events.

Below is an example configuration that uses Vector's built-in cardinality limiting:

```toml
# Receive metrics from Prometheus via remote write
[sources.prometheus_in]
type = "prometheus_remote_write"
address = "0.0.0.0:9090"

# Limit the number of labels by dropping tags
[transforms.limit_cardinality]
type = "tag_cardinality_limit"
inputs = ["prometheus_in"]
limit_exceeded_action = "drop_tag"
mode = "exact"
value_limit = 250
```

Here, the `tag_cardinality_limit` transform restricts the number of tags (labels) to 250, dropping
any tags that exceed that limit. Inside an observability pipeline, using Vector in this way would
likely reduce the indexing and storage burden on any downstream services handling your Prometheus
metrics.

## Use cases

### Limit label cardinality

### Transform Prometheus metrics

You can apply arbitrary modifications to your Prometheus metrics using [Vector Remap Language][vrl]
(VRL) inside a [`remap`][remap] transform. A common use case is adding metadata to the metric.
Here's an example:

```toml
[transforms.add_tags]
type = "remap"
inputs = ["prometheus_in"]
source = '''
.tags = {
  "env": "staging"
}
'''
```

## Example configurations

### Scrape, limit cardinality, and write to Prometheus remote write

```toml
# Scrape metrics from a Prometheus endpoint every 15 seconds
[sources.prometheus_in]
type = "prometheus_scrape"
endpoints = ["http://my-prometheus-server:9090/metrics"]
scrape_interval_secs = 15

# Limit the number of labels
[transforms.limit_cardinality]
type = "tag_cardinality_limit"
inputs = ["prometheus_in"]
limit_exceeded_action = "drop_tag"
mode = "exact"

# Write to a Prometheus endpoint
[sinks.prometheus_out]
type = "prometheus_remote_write"
inputs = ["limit_cardinality"]
endpoint = "https://saas-service.io"
```

[cardinality]: https://www.robustperception.io/cardinality-is-key
[cncf]: https://cncf.io
[drop]: /docs/reference/configuration/transforms/tag_cardinality_limit/#limit_exceeded_action
[exporter]: https://prometheus.io/docs/instrumenting/exporters
[exporter_sink]: /docs/reference/configuration/sinks/prometheus_exporter
[prometheus]: https://prometheus.io
[remap]: /docs/reference/configuration/transforms/remap
[remote_write]: https://prometheus.io/docs/practices/remote_write
[remote_endpoints]: https://prometheus.io/docs/operating/integrations/#remote-endpoints-and-storage
[remote_write_sink]: /docs/reference/configuration/sinks/prometheus_remote_write
[remote_write_source]: /docs/reference/configuration/sources/prometheus_remote_write
[scrape_source]: /docs/reference/configuration/sources/prometheus_scrape
[sinks]: /docs/reference/configuration/sinks
[sources]: /docs/reference/configuration/sources
[tag_cardinality_limit]: /docs/reference/configuration/transforms/tag_cardinality_limit
[vrl]: /docs/reference/vrl
