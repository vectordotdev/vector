---
last_modified_on: "2020-04-13"
$schema: "/.meta/.schemas/highlights.json"
title: "Whitelist & Blacklist Fields When Encoding"
description: "More control over which fields are included when encoding"
author_github: "https://github.com/binarylogic"
pr_numbers: [1915]
release: "0.9.0"
hide_on_release_notes: true
tags: ["type: new feature", "domain: sinks"]
---

We've added the ability to whitelist and blacklist fields during the encoding
process within [sinks][docs.sinks]. This is useful if you have metadata fields
that you do not want to send downstream. For example, you might have an
`application_id` fields that you use for partitioning, but you don't want to
include it inthe actual data since it would be duplicative.

To use this feature see the new `encoding` options for each sink. For example,
the [`clickhouse` sink's `encoding` option][docs.sinks.clickhouse#encoding].


[docs.sinks.clickhouse#encoding]: /docs/reference/sinks/clickhouse/#encoding
[docs.sinks]: /docs/reference/sinks/
