---
last_modified_on: "2020-04-13"
$schema: "/.meta/.schemas/highlights.json"
title: "The `datadog` sink has been renamed to `datadog_metrics`"
description: "This ensures that naming is consistent for the upcoming `datadog_logs` sink"
author_github: "https://github.com/binarylogic"
pr_numbers: [1314]
release: "0.6.0"
hide_on_release_notes: false
tags: ["type: breaking change", "domain: sinks", "sink: datadog_metrics"]
---

The `datadog` sink has been renamed to `datadog_metrics` to make way for the
upcoming `datadog_logs` sink.

## Upgrade Guide

Make the following changes in your `vector.toml` file:

```diff title="vector.toml"
 [sinks.datadog]
-  type = "datadog"
+  type = "datadog_metrics"
```

That's it!



