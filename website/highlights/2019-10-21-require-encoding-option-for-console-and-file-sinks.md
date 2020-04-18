---
last_modified_on: "2020-04-15"
$schema: "/.meta/.schemas/highlights.json"
title: "Require `encoding` option for console and file sinks"
description: "The `encoding` option is now required for these sinks"
author_github: "https://github.com/binarylogic"
pr_numbers: [1033]
release: "0.6.0"
hide_on_release_notes: false
tags: ["type: breaking change", "domain: sinks", "sink: console", "sink: file"]
---

The dynamic `encoding` concept in Vector was confusing users, so we've made
it required and explicit. Simply add `encoding = "json"` to your `console` and
`file` sinks.

## Upgrade Guide

Make the following changes in your `vector.toml` file:

```diff title="vector.toml"
 [sinks.console]
   type = "console"
+  encoding = "json"

 [sinks.file]
   type = "file"
+  encoding = "json"
```

That's it!



