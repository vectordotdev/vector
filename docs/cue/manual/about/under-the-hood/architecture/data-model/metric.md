---
title: Metric Event
description: Vector's internal metric data model.
---

<SVG src="/optimized_svg/data-model-metric_1513_942.svg" />

## Description

A Vector metric event represents a numerical operation
performed on a time series. Unlike other tools, metrics
in Vector are first class citizens, they are not represented
as structured logs. This makes them interoperable with
various metrics services without the need for any
transformation.

Vector's metric data model favors accuracy and correctness over
ideological purity. Therefore, Vector's metric types are a
conglomeration of various metric types found in the wild, such as
Prometheus and Statsd. This ensures metric data is _correctly_
interoperable between systems.
