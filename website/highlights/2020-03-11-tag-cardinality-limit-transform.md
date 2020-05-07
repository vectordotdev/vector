---
last_modified_on: "2020-04-16"
$schema: "/.meta/.schemas/highlights.json"
title: "New Tag Cardinality Limit Transform"
description: "Protect downstream metrics storages from runaway metrics tags"
author_github: "https://github.com/binarylogic"
pr_numbers: [1959]
release: "0.9.0"
hide_on_release_notes: false
tags: ["type: new feature", "domain: transforms", "transform: tag_cardinality_limit"]
---

import CodeExplanation from '@site/src/components/CodeExplanation';

High cardinality metric tags can severy disrupt downstream metrics storages. To
protet against this we built a new
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

<CodeExplanation>

* The `limit_exceeded_action` described the behavior when the `value_limit` is reached.
* The `mode` enables you to switch between `exact` and `probabilistic` algorithms to trade performance for memory efficiency.
* The `value` limit allows you to select exactly how many unique tag values you're willing to accept.

</CodeExplanation>

More to come! This feature is part of our [best-in-class operator
UX][urls.milestone_39] initiative.


[docs.transforms.tag_cardinality_limit]: /docs/reference/transforms/tag_cardinality_limit/
[urls.milestone_39]: https://github.com/timberio/vector/milestone/39
