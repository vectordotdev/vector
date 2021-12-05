---
date: "2020-04-13"
title: "New Swimlanes Transform"
description: "Split log streams with ease"
authors: ["binarylogic"]
pr_numbers: [1785]
release: "0.8.0"
hide_on_release_notes: false
badges:
  type: "new feature"
  domains: ["transforms"]
  transforms: ["swimlanes"]
---

The new [`swimlanes` transform][docs.transforms.swimlanes] makes it much easier
to configure conditional branches of transforms and sinks. For example, you can
easily create [if/else pipelines][docs.transforms.swimlanes#examples].

```toml title="vector.toml"
[transforms.lanes]
  types = "swimlanes"

  [transforms.my_transform_id.lanes.errors]
    "level.eq" = "error"

  [transforms.my_transform_id.lanes.not_errors]
    "level.neq" = "error"
```

Remember to occasionally let your branches mingle so that they don't completely
lose touch.

[docs.transforms.swimlanes#examples]: /docs/reference/configuration/transforms/route/#examples
[docs.transforms.swimlanes]: /docs/reference/configuration/transforms/route/
