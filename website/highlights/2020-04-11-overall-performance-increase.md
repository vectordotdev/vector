---
last_modified_on: "2020-04-14"
$schema: "/.meta/.schemas/highlights.json"
title: "~36% Performance Improvement"
description: "Significant performance gains for all Vector users"
author_github: "https://github.com/binarylogic"
pr_numbers: [2295, 2296]
release: "nightly"
importance: "high"
tags: ["type: performance"]
---

After some hard profiling work, we're pleased to announce that Vector is now
~36% faster for most use cases. The improvements are two-fold:

## 1. Data Model Improvements

For you Rustaceons, we were able to gain an easy ~8% improvement on
throughput by swapping our internal use of `atom`s with `string`s. See
[PR#2295][urls.pr_2295] for more info.

## 2. Path Notation Caching

Within Vector's configuration files we have a concept called [field path
notation][docs.reference.field-path-notation]. This is a fancy name for how
users can target log fields. For example:

```toml title="vector.toml"
[transforms.my_transform_id]
  # General
  type = "add_fields" # required
  inputs = ["my-source-id"] # required

  # Fields
  fields."string_field" = "string value"
  fields."parent.child_field" = "child_value"
  fields."array[0]" = "array_value"
  fields."key\. with. periods\." = "periods_value"
```

As you can see above, this syntax unlocks some advanced use cases that most
users of Vector arent't tapping into. Such as accessing array indices and
keys with escaped characters. And it turns out parsing all of this is expensive!
To improve this we implements a couple of optimizations:

1. Detect if advanced parsing is necessary, and if not, use a fast path.
2. Cache parsing on bootm.

These 2 changes netted about a ~28% improvement. Checkout [PR#2296][urls.pr_2296]
for more info.


[docs.reference.field-path-notation]: /docs/reference/field-path-notation/
[urls.pr_2295]: https://github.com/timberio/vector/pull/2295
[urls.pr_2296]: https://github.com/timberio/vector/pull/2296
