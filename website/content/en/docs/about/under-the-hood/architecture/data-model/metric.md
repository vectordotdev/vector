---
title: Metric events
weight: 2
tags: ["metrics", "events", "schema"]
---

{{< svg "img/data-model-metric.svg" >}}

A **metric event** in Vector represents a numerical operation performed on a time series. In Vector, unlike in other tools, metrics are first-class citizens. They are *not* represented as [logs]. This makes them interoperable with various metrics services without the need for any transformation.

Vector's metric data model favors accuracy and correctness over ideological purity. Vector's metric types are thus an agglomeration of various metric types found in the wild, such as [Prometheus] and [Statsd]. This ensures that metrics are *correctly* interoperable between systems.

## Schema

{{< config/metric-schema >}}

[logs]: /docs/about/under-the-hood/architecture/data-model/log
[prometheus]: https://prometheus.io
[statsd]: https://github.com/statsd/statsd
