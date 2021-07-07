---
date: "2020-07-13"
title: "The `splunk_hec` source's `host_field` option has been renamed to `host_key`"
description: "This change ensures that the `host_key` option is consistent across all sources"
authors: ["binarylogic"]
pr_numbers: [2037]
release: "0.9.0"
hide_on_release_notes: true
badges:
  type: "breaking change"
  domains: ["sinks"]
  sinks: ["splunk_hec"]
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
