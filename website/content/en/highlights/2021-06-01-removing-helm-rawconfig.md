---
date: "2021-06-01"
title: "Deprecating Helm `rawConfig` option"
description: "The `rawConfig` option in the Vector Helm charts will be fully deprecated in an upcoming release"
authors: ["spencergilbert"]
pr_numbers: [7671]
release: "0.14.0"
hide_on_release_notes: false
badges:
  type: "deprecation"
  platforms: ["kubernetes"]
  domains: ["config"]
---

With the release of Vector 0.14.0, we are announcing the planned deprecation of the `rawConfig` option
in our Helm charts. This will remove a source of complexity in our chart logic, and is a step towards
using configuration provided in the `values.yaml` without converting them for the `managed.toml` config file.

## Upgrade Guide

The values that were in `rawConfig` in TOML format now need to be moved up a level and written as YAML.
Please refer to the `vector.yaml` on each component's documentation page for specific configuration examples.

Below is an example of how to transition your `values.yaml` file:

```diff title="values.yaml"
   sources:
     dummy:
       type: "generator"
-      rawConfig: |
-        format = "shuffle"
-        lines = ["Hello world"]
-        interval = 60 # once a minute
+      format: "shuffle"
+      lines: ["Hello world"]
+      interval: 60 # once a minute

   sinks:
     stdout:
       type: "console"
       inputs: ["dummy"]
-      rawConfig: |
-        target = "stdout"
-        encoding.codec = "json"
+      target: "stdout"
+      encoding.codec: "json"
```
