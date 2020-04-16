---
last_modified_on: "2020-04-13"
$schema: "/.meta/.schemas/highlights.json"
title: "New Tag Cardinality Limit Transform"
description: "Protect downstream metrics storages from high cardinality tags"
author_github: "https://github.com/binarylogic"
pr_numbers: [1959]
release: "nightly"
hide_on_release_notes: false
tags: ["type: new feature", "domain: transforms", "transform: tag_cardinality_limit"]
---

High cardinality labels got you up at night...literally? Check out our new
[`tag_cardinality_limit` transform][docs.transforms.tag_cardinality_limit].
It protects your metrics storage from label misuse and let's your sleep at
night.

More to come! This feature is part of our [best-in-class operator
UX][urls.milestone_39] initiative.


[docs.transforms.tag_cardinality_limit]: /docs/reference/transforms/tag_cardinality_limit/
[urls.milestone_39]: https://github.com/timberio/vector/milestone/39
