---
date: "2021-02-16"
title: "Remap support for the `reduce` transform (multi-line logs)"
short: "Reduce in Remap"
description: "Use VRL to specify conditions for reducing multi-log lines into a single log event"
authors: ["lucperkins"]
pr_numbers: []
release: "0.12.0"
hide_on_release_notes: false
badges:
  type: "enhancement"
  transforms: ["reduce"]
---

The [`reduce`][reduce] transform for Vector enables you to reduce multiple log
events into a single event, which is useful for logs that are split across
multiple lines, such as JVM stack traces. With `reduce` you can specify both a
merge strategy for the events and a condition for specifying when a group of
events either starts or ends (that group is then collapsed, i.e. *reduced* into
a single event).

Previously, the `reduce` transform required you to specify conditions using
`check_fields`. But now you can use Vector Remap Language expressions in the
[`remap`][remap] transform to specify those conditions. This should make using
`reduce` more natural and expressive.

## Example

The example configuration below shows the same `reduce` transform using the old
system (`check_fields`) and the new system (`remap`):

```diff
 [transforms.merge_stack_trace]
 type = "reduce"
 inputs = ["jvm-logs"]
 merge_strategies.message = "concat_newline"

 # Using check_fields
-starts_when.type = "check_fields"
-starts_when.message.regex = "/^\\w.*/"
-starts_when.severity.eq = "info"

 # Using remap
+starts_when.type = "remap"
+starts_when.source = 'match(string!(.message), r'^\\w.*') && .severity == "info"'
```

[reduce]: /docs/reference/configuration/transforms/reduce
[remap]: /docs/reference/configuration/transforms/remap
