---
date: "2020-07-13"
title: "Require `encoding` option for console and file sinks"
description: "The `encoding` option is now required for these sinks"
authors: ["binarylogic"]
pr_numbers: [1033]
release: "0.6.0"
hide_on_release_notes: false
badges:
  type: "breaking change"
  domains: ["sinks"]
  sinks: ["console", "file"]
---

The dynamic `encoding` concept in Vector was confusing users, so we've made
it required and explicit. Simply add `encoding.codec = "json"` to your `console`
and `file` sinks.

## Upgrade Guide

Make the following changes in your `vector.toml` file:

```diff title="vector.toml"
 [sinks.console]
   type = "console"
+  encoding.codec = "json"

 [sinks.file]
   type = "file"
+  encoding.codec = "json"
```

That's it!
