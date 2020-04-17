---
last_modified_on: "2020-04-16"
$schema: "/.meta/.schemas/highlights.json"
title: "The `gcp_stackdriver_logging` sink has been renamed to `gcp_stackdriver_logs`"
description: "This brings the sink naming inline with consistent naming pattern"
author_github: "https://github.com/binarylogic"
pr_numbers: [2121]
release: "0.9.0"
hide_on_release_notes: false
tags: ["type: breaking change","domain: sinks","sink: splunk_hec"]
---

We've renamed the `gcp_stackdriver_logging` sink to `gcp_stackdriver_logs` to
bring it inline with consistent naming patterns. Migration is easy.

## Upgrade Guide

```diff title="vector.toml"
 [sinks.stackdriver]
-  type = "gcp_stackdriver_logging"
+  type = "gcp_stackdriver_logs"
```



