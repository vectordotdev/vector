---
date: "2022-08-08"
title: "Internal metrics cardinality metric change"
description: "Change to the cardinality metric emitted by `internal_metrics`."
authors: ["bruceg"]
pr_numbers: [13854]
release: "0.24.0"
hide_on_release_notes: false
badges:
  type: enhancement
---

The `internal_metrics` source has been emitting a metric indicating the number of distinct metrics
in its internal registry. This currently has the name `internal_metrics_cardinality_total` and is a
counter type metric.

With the introduction of the capability of expiring metrics from the internal registry, it is no
longer accurate to call this a counter as its value may fluctuate during the running of vector. As
such, a new metric called `internal_metrics_cardinality` of type gauge has been added. The existing
metric described above is now deprecated and will be removed in a future version.

See the
[documentation](/docs/reference/configuration/sources/internal_metrics/#internal_metrics_cardinality)
for additional details.
