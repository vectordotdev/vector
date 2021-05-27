---
last_modified_on: "2021-06-01"
$schema: ".schema.json"
title: "Helm `rawConfig` removal"
description: "The `rawConfig` option in the Vector Helm charts has been fully deprecated"
author_github: "https://github.com/spencergilbert"
pr_numbers: [7608]
release: "0.14.0"
hide_on_release_notes: false
tags: ["type: breaking change"]
---

In the Vector 0.14.0 Helm charts, we no longer support the `rawConfig` key for component configuration.
This reduces complexity and improves maintainability for our charts, as well as eliminating bugs
related to the templating of `rawConfigs`.

## Upgrade Guide

Below is an example of how to make the required changes in your `values.yaml` file:

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
-        encoding = "json"
+      target: "stdout"
+      encoding: "json"
```
