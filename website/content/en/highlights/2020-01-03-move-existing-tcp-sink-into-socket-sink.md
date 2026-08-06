---
date: "2020-07-13"
title: "Rename existing `tcp` sink to `socket` sink"
description: "This renames the existing `tcp` sink to `socket`"
authors: ["binarylogic"]
pr_numbers: [1404]
release: "0.7.0"
hide_on_release_notes: false
badges:
  type: "breaking change"
  domains: ["sinks"]
  sinks: ["tcp"]
---

The `tcp` sink has been renamed to `socket`. This is part of an overall effort
to simplify our sinks in a manner where they can easily be "wrapped" as the
foundation for upcoming sinks.

## Upgrade Guide

```diff title="vector.toml"
 [sources.my_tcp_sink]
-  type = "tcp"
+  type = "socket"
   address = "92.12.333.224:5000"
+  mode = "tcp"
```
