---
date: "2020-07-13"
title: "New Encoding Options"
description: "Vector now lets you whitelist, blacklist, and format fields when events are encoded"
authors: ["binarylogic"]
pr_numbers: [1915]
release: "0.9.0"
hide_on_release_notes: false
badges:
  type: "enhancement"
  domains: ["sinks"]
---

Vector has deprecated the root-level `encoding` option in favor of new
`encoding.*` sub-options:

- `encoding.only_fields` - Encode only the fields listed.
- `encoding.except_fields` - Encode all fields except the ones listed.
- `encoding.codec` - The codec to use (ex: `json`).
- `encoding.timestamp_format` - Customize how timestamps are serialized.

## Upgrade Guide

Upgrading is easy:

```toml title="vector.toml"
 [sinks.my-sink]
   type = "..."
-  encoding = "json"
+  encoding.codec = "json"
+  encoding.except_fields = ["_meta"] # optional
+  encoding.timestamp_format = "rfc3339" # optional
```
