---
date: "2020-07-13"
title: "The `datadog` sink has been renamed to `datadog_metrics`"
description: "This ensures that naming is consistent for the upcoming `datadog_logs` sink"
authors: ["binarylogic"]
pr_numbers: [1314]
release: "0.6.0"
hide_on_release_notes: false
badges:
  type: "breaking change"
  domains: ["sinks"]
  sinks: ["datadog_metrics"]
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
