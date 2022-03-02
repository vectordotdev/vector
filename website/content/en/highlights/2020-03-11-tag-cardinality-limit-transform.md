---
date: "2020-04-16"
title: "New Tag Cardinality Limit Transform"
description: "Protect downstream metric storage systems from metric tag explosion"
authors: ["binarylogic"]
pr_numbers: [1959]
release: "0.9.0"
hide_on_release_notes: false
badges:
  type: "new feature"
  domains: ["transforms"]
  transforms: ["tag_cardinality_limit"]
---

High cardinality metric tags can severely disrupt downstream metrics storages. To
protect against this we built a new
[`tag_cardinality_limit` transform][docs.transforms.tag_cardinality_limit].

## Getting Started

Getting started is easy. Simply add this component to your pipeline:

```toml title="vector.toml"
[transforms.tag_protection]
  type = "tag_cardinality_limit"
  inputs = ["my-source-id"]
  limit_exceeded_action = "drop_tag"
  mode = "exact"
  value_limit = 500
```

{{< success >}}

- The `limit_exceeded_action` described the behavior when the `value_limit` is reached.
- The `mode` enables you to switch between `exact` and `probabilistic` algorithms to trade performance for memory efficiency.
- The `value` limit allows you to select exactly how many unique tag values you're willing to accept.
{{< /success >}}

More to come! This feature is part of our [best-in-class operator
UX][urls.milestone_39] initiative.

[docs.transforms.tag_cardinality_limit]: /docs/reference/configuration/transforms/tag_cardinality_limit/
[urls.milestone_39]: https://github.com/vectordotdev/vector/milestone/39
