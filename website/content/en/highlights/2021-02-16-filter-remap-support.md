---
title: "Remap support for the `filter` transform"
description: "Use VRL to specify conditions for filtering events in a stream"
date: "2021-02-16"
authors: ["lucperkins"]
pr_numbers: []
release: "0.12.0"
hide_on_release_notes: false
badges:
  type: "enhancement"
  transforms: ["filter"]
---

The [`filter`][filter] transform for Vector enables you to winnow down a stream
of events to only those that match a specified condition.

Previously, the `filter` transform required you to specify conditions using
`check_fields`. But now you can use Vector Remap Language expressions in the
[`remap`][remap] transform to specify those conditions. This should make using
`filter` more natural and expressive.

## Example

The example configuration below shows the same `filter` transform using the old
system (`check_fields`) and the new system (`remap`):

```yaml
transforms:
  filter_out_non_critical:
    type: "filter"
    inputs: ["http-server-logs"]

    # Using check_fields
    condition:
      type: "check_fields"
      message:
        status_code:
          ne: 200
        severity:
          ne: "info"
          # ne: "debug"

    # Using remap
    condition:
      type: "remap"
      source: '.status_code != 200 && !includes(["info", "debug"], .severity)'
```

[filter]: /docs/reference/configuration/transforms/filter
[remap]: /docs/reference/configuration/transforms/remap
