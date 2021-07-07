---
date: "2020-07-13"
title: "The `gcp_stackdriver_logging` sink has been renamed to `gcp_stackdriver_logs`"
description: "This brings the sink naming inline with consistent naming pattern"
authors: ["binarylogic"]
pr_numbers: [2121]
release: "0.9.0"
hide_on_release_notes: false
badges:
  type: "breaking change"
  domains: ["sinks"]
  sinks: ["splunk_hec"]
---

We've renamed the `gcp_stackdriver_logging` sink to `gcp_stackdriver_logs` to
bring it in-line with consistent naming patterns. Migration is easy.

## Upgrade Guide

```diff title="vector.toml"
 [sinks.stackdriver]
-  type = "gcp_stackdriver_logging"
+  type = "gcp_stackdriver_logs"
```
