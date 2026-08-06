---
date: "2020-07-13"
title: "Merge existing `tcp` and `udp` sources into a single `socket` source"
description: "We've simplified our socket based sources into a single `socket` source"
authors: ["binarylogic"]
pr_numbers: [1485]
release: "0.7.0"
hide_on_release_notes: false
badges:
  type: "breaking change"
  domain: ["sources"]
  sources: ["tcp", "udp"]
---

The `tcp` and `udp` sources no longer exist and have been merged into a new
`socket` type.

## Upgrade Guide

Migration is straight forward, simply change the `type` to `socket` and add the
field `mode` to match the socket type (`tcp` or `udp`):

```diff title="vector.toml"
 [sources.my_tcp_source]
-  type = "tcp"
+  type = "socket"
   address = "0.0.0.0:9000"
+  mode = "tcp"
```
