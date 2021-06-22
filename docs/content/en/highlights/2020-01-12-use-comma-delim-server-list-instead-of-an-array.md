---
date: "2020-07-13"
title: "Use comma delim server list in `kafka` sink"
description: "This change achieve consistency with our `kafka` source and other Kafka clients"
authors: ["binarylogic"]
pr_numbers: [1502]
release: "0.7.0"
hide_on_release_notes: false
badges:
  type: "breaking change"
  domains: ["sinks"]
  sinks: ["kafka"]
---

The `kafka` sink field `bootstrap_servers` has been changed from an array to a
string, expecting a comma separated list of bootstrap servers similar to the
`kafka` source.

## Upgrade Guide

```diff title="vector.toml"
 [sinks.my_sink_id]
   type = "kafka"
   inputs = ["my-source-id"]
-  bootstrap_servers = ["10.14.22.123:9092", "10.14.23.332:9092"]
+  bootstrap_servers = "10.14.22.123:9092,10.14.23.332:9092"
```
