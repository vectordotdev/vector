---
last_modified_on: "2020-04-16"
$schema: "/.meta/.schemas/highlights.json"
title: "The `splunk_hec` source's `host_field` option has been renamed to `host_key`"
description: "This change ensures that the `host_key` option is consistent across all sources"
author_github: "https://github.com/binarylogic"
pr_numbers: [2037]
release: "0.9.0"
hide_on_release_notes: true
tags: ["type: breaking change","domain: sinks","sink: splunk_hec"]
---

We've renamed the [`splunk_hec` source's] `host_field` option to `host_key`.
This ensures that the `host_key` option is consistent across all sources.

## Upgrade Guide

```diff title="vector.toml"
 [sources.splunk]
   type = "splunk_hec"
-  host_field = "host"
+  host_key = "host"
```



