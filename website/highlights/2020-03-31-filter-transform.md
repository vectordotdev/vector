---
last_modified_on: "2020-04-16"
$schema: "/.meta/.schemas/highlights.json"
title: "New Filter Transform"
description: "Filter and route your logs based on defined conditions"
author_github: "https://github.com/binarylogic"
pr_numbers: [2088]
release: "0.9.0"
hide_on_release_notes: false
tags: ["type: new feature", "domain: transforms", "transform: filter"]
---

We recently introduced a concept of conditions, which you can see in our
[`swimlanes` transform][docs.transforms.swimlanes] as well as our [unit
tests feature][docs.reference.tests]. This paved the way for a new `filter`
transform, allowing you to filter events based on a set of conditions. This
is replaces our old `field_filter` transform since it is much more expressive.

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


[docs.reference.tests]: /docs/reference/tests/
[docs.transforms.filter]: /docs/reference/transforms/filter/
[docs.transforms.swimlanes]: /docs/reference/transforms/swimlanes/
