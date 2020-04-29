---
last_modified_on: "2020-04-13"
$schema: "/.meta/.schemas/highlights.json"
title: "More Condition Predicates"
description: "More options when filtering and routing events"
author_github: "https://github.com/binarylogic"
pr_numbers: [1997, 2183, 2198]
release: "0.9.0"
hide_on_release_notes: true
tags: ["type: enhancement", "domain: config", "domain: transforms", "transform: filter", "transform: swimlanes"]
---

Vector has a concept "conditions" that are used to qualify events. For example,
this is used in Vector's [unit testing feature][guides.unit-testing],
[`swimlanes` transform][docs.transforms.swimlanes], and
[`filter` transform][docs.transforms.filter]. This change adds new predicates
that enable powerful matching and condition expression. Specifically, the
following predicates were added:

* `begins_with`
* `contains`
* `ends_with`
* `is_log`
* `is_metric`
* `regex`

## Example

For example, you can filter all messages that contain the `error` term with
the new `contains` predicate:

```toml
[transforms.errors]
  type = "filter"
  condition."message.cotnain" = "error"
```

The world is your oyster.


[docs.transforms.filter]: /docs/reference/transforms/filter/
[docs.transforms.swimlanes]: /docs/reference/transforms/swimlanes/
[guides.unit-testing]: /guides/advanced/unit-testing/
