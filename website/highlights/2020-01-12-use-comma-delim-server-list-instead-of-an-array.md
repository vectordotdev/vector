---
last_modified_on: "2020-04-13"
$schema: "/.meta/.schemas/highlights.json"
title: "Use comma delim server list in `kafka` sink"
description: "This change achieve consistency with our `kafka` source and other Kafka clients"
author_github: "https://github.com/binarylogic"
pr_numbers: [1502]
release: "0.7.0"
hide_on_release_notes: false
tags: ["type: breaking change","domain: sinks","sink: kafka"]
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



