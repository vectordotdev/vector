---
date: "2020-04-13"
title: "Whitelist and Blacklist Fields When Encoding"
description: "More control over which fields are included when encoding"
authors: ["binarylogic"]
pr_numbers: [1915]
release: "0.9.0"
hide_on_release_notes: true
badges:
  type: "new feature"
  domains: ["sinks"]
---

We've added the ability to white-list and blacklist fields during the encoding
process within [sinks][docs.sinks]. This is useful if you have metadata fields
that you do not want to send downstream. For example, you might have an
`application_id` fields that you use for partitioning, but you don't want to
include it in the actual data since it would be duplicative.

To use this feature see the new `encoding` options for each sink. For example,
the [`clickhouse` sink's `encoding` option][docs.sinks.clickhouse#encoding].

[docs.sinks.clickhouse#encoding]: /docs/reference/configuration/sinks/clickhouse/#encoding
[docs.sinks]: /docs/reference/configuration/sinks/
