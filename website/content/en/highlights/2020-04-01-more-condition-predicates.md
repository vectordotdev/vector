---
date: "2020-04-13"
title: "More Condition Predicates"
description: "More options when filtering and routing events"
authors: ["binarylogic"]
pr_numbers: [1997, 2183, 2198]
release: "0.9.0"
hide_on_release_notes: true
badges:
  type: "enhancement"
  domains: ["config", "transforms"]
  transforms: ["filter", "swimlanes"]
---

Vector has a concept "conditions" that are used to qualify events. For example,
this is used in Vector's [unit testing feature][guides.unit-testing],
[`swimlanes` transform][docs.transforms.swimlanes], and
[`filter` transform][docs.transforms.filter]. This change adds new predicates
that enable powerful matching and condition expression. Specifically, the
following predicates were added:

- `begins_with`
- `contains`
- `ends_with`
- `is_log`
- `is_metric`
- `regex`

## Example

For example, you can filter all messages that contain the `error` term with
the new `contains` predicate:

```toml
[transforms.errors]
  type = "filter"
  condition."message.contain" = "error"
```

The world is your oyster.

[docs.transforms.filter]: /docs/reference/configuration/transforms/filter/
[docs.transforms.swimlanes]: /docs/reference/configuration/transforms/route/
[guides.unit-testing]: /guides/level-up/unit-testing/
