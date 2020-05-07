---
last_modified_on: "2020-04-13"
$schema: "/.meta/.schemas/highlights.json"
title: "The `splunk_hec` sink does not index fields by default"
description: "This gives you full control over which fields are indexed"
author_github: "https://github.com/binarylogic"
pr_numbers: [1537]
release: "0.7.0"
hide_on_release_notes: false
tags: ["type: breaking change","domain: sinks","sink: splunk_hec"]
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



