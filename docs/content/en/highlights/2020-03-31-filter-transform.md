---
date: "2020-04-16"
title: "New Filter Transform"
description: "Filter and route your logs based on defined conditions"
authors: ["binarylogic"]
pr_numbers: [2088]
release: "0.9.0"
hide_on_release_notes: false
badges:
  type: "new feature"
  domains: ["transforms"]
  transforms: ["filter"]
---

We recently introduced a concept of conditions, which you can see in our
[`route` transform][docs.transforms.route] as well as our unit
tests feature. This paved the way for a new `filter` transform, allowing you to
filter events based on a set of conditions. This replaces our old `field_filter`
transform since it is much more expressive.

## Get Started

```toml title="vector.toml"
[transforms.haproxy_errors]
  # General
  type = "filter"
  inputs = ["my-source-id"]

  # Conditions
  condition."level.eq" = "error"
  condition."service.eq" = "haproxy"
```

Check out the [docs][docs.transforms.filter] for a fill list of available
conditions.

[docs.transforms.filter]: /docs/reference/configuration/transforms/filter/
[docs.transforms.route]: /docs/reference/configuration/transforms/route/
