---
date: "2021-01-20"
title: "Wildcards are now supported in component names"
description: "Wildcards allow for dynamic Vector topologies"
authors: ["binarylogic"]
featured: false
pr_numbers: [6170]
release: "0.12.0"
hide_on_release_notes: false
badges:
  type: "enhancement"
  domains: ["topology"]
---

[PR 6170][pr_6170] introduced wildcards when referencing component names in the `inputs` option. This allows you to build
dynamic topologies. This feature comes with one limitation: the wildcard must be at the end of the string.

```toml
[sources.app1_logs]
type = "file"
includes = ["/var/log/app1.log"]

[sources.app2_logs]
type = "file"
includes = ["/var/log/app.log"]

[sources.system_logs]
type = "file"
includes = ["/var/log/system.log"]

[sinks.app_logs]
type = "datadog_logs"
inputs = ["app*"]

[sinks.archive]
type = "aws_s3"
inputs = ["app*", "system_logs"]
```

[pr_6170]: https://github.com/vectordotdev/vector/pull/6170
