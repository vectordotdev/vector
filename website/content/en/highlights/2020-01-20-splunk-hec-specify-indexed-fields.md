---
date: "2020-07-13"
title: "The `splunk_hec` sink does not index fields by default"
description: "This gives you full control over which fields are indexed"
authors: ["binarylogic"]
pr_numbers: [1537]
release: "0.7.0"
hide_on_release_notes: false
badges:
  type: "breaking change"
  domains: ["sinks"]
  sinks: ["splunk_hec"]
---

There is no longer a distinction within Vector between explicit and implicit
event fields. All fields are now implicit and therefore the `splunk_hec` sink
will _not_ index any fields by default.

## Upgrade Guide

In order to mark desired fields as indexed you can use the optional
configuration option `indexed_fields`:

```toml title="vector.toml"
 [sinks.my_sink_id]
   type = "splunk_hec"
   inputs = ["my-source-id"]
+  indexed_fields = ["foo", "bar"]
```
