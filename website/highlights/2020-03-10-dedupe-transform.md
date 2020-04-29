---
last_modified_on: "2020-04-16"
$schema: "/.meta/.schemas/highlights.json"
title: "New Dedupe Transform"
description: "Shed duplicate logs"
author_github: "https://github.com/binarylogic"
pr_numbers: [1848]
release: "0.9.0"
hide_on_release_notes: false
tags: ["type: new feature", "domain: sources", "source: vector"]
---

import CodeExplanation from '@site/src/components/CodeExplanation';

For certain use cases log deduplication can be a useful tool. Not only does
this promote your data integrity, but it can help protect against upstream
mistakes that accidentally doplicate logs. This mistake can easily double
(or more!) your log volume. To protect against this you can use our new
[`dedupe` transform][docs.transforms.dedupe].

## Get Started

Simply add the transform to your pipeline:

```toml
[transforms.my_transform_id]
  # General
  type = "dedupe" # required
  inputs = ["my-source-id"] # required

  # Fields
  fields.match = ["timestamp", "host", "message"] # optional, default
```

<CodeExplanation>

* The `fields.match` option lets you control which fields are compared to determine if events are equal.

</CodeExplanation>


[docs.transforms.dedupe]: /docs/reference/transforms/dedupe/
