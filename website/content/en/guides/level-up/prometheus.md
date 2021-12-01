---
title: Using Vector with Prometheus
short: Prometheus
description: Replace this later
author_github: https://github.com/lucperkins
domain: metrics
weight: 5
tags: ["prometheus", "metrics", "level up", "guides", "guide"]
---

[Prometheus] is a popular open source metrics platform under the auspices of the [Cloud Native
Computing Foundation][cncf] (CNCF).

You can use Vector in conjunction with Prometheus in a wide variety of ways. In this guide, we'll
cover several use cases.

## Components

Vector provides several [sources](#sources) and [sinks](#sinks) that directly pertain to Prometheus,
as well as one [transform](#transforms).

### Prometheus sources {#sources}

Vector has two Promtheus-related [sources] that you can use in your observability data pipelines:

Source | What it enables
:------|:---------------
[`prometheus_remote_write`][prometheus_remote_write_source] | Use Vector as a Prometheus [remote write][remote] endpoint
[`prometheus_scrape`][prometheus_scrape_source] | Use Vector to scrape metrics directly from any [Prometheus exporter][exporter].

### Prometheus sinks {#sinks}

Vector offers two Prometheus-related [sinks] that you can use in your observability data pipelines:

Sink | What it enables
:----|:---------------
[`prometheus_remote_write`][prometheus_remote_write_sink] | Ship metrics to any [Prometheus remote write][remote_write] endpoint, which includes a wide variety of [storage systems and services][remote_endpoints].
[`prometheus_exporter`][prometheus_exporter_sink] | Make Vector essentially act like any standard [Prometheus exporter][exporter] that exposes Prometheus metrics via HTTP.

### Prometheus-related transforms {#transforms}

Although Vector doesn't provide any Prometheus-specific transforms, one transform that can be quite
useful when used in conjunction with Prometheus is the
[`tag_cardinality_limit`][tag_cardinality_limit] transform, which enables you to limit the number of
tags attached to metric events.

[High label cardinality][cardinality] in metric labels is a common problem when using Prometheus
(the terms "label" and "tag" are synonymous in this context)

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
[exporter]: https://prometheus.io/docs/instrumenting/exporters
[prometheus]: https://prometheus.io
[prometheus_exporter_sink]: /docs/reference/configuration/sinks/prometheus_exporter
[prometheus_remote_write_sink]: /docs/reference/configuration/sinks/prometheus_remote_write
[prometheus_remote_write_source]: /docs/reference/configuration/sources/prometheus_remote_write
[prometheus_scrape_source]: /docs/reference/configuration/sources/prometheus_scrape
[remote_write]: https://prometheus.io/docs/practices/remote_write
[remote_endpoints]: https://prometheus.io/docs/operating/integrations/#remote-endpoints-and-storage
[sinks]: /docs/reference/configuration/sinks
[sources]: /docs/reference/configuration/sources
[tag_cardinality_limit]: /docs/reference/configuration/transforms/tag_cardinality_limit
