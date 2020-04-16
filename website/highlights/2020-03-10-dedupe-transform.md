---
last_modified_on: "2020-04-13"
$schema: "/.meta/.schemas/highlights.json"
title: "New Dedupe Trasnform"
description: "Shed duplicate logs"
author_github: "https://github.com/binarylogic"
pr_numbers: [1848]
release: "nightly"
hide_on_release_notes: false
tags: ["type: new feature", "domain: sources", "source: vector"]
---

For certain use cases log deduplication can be a useful tool. Not only does
this promote your data integrity, but it can help protect against upstream
mistakes that accidentally doplicate logs. This mistake can easily double
(or more!) your log volume. To protect against this you can use our new
[`dedupe` transform][docs.transforms.dedupe].


[docs.transforms.dedupe]: /docs/reference/transforms/dedupe/
