---
last_modified_on: "2020-04-16"
$schema: "/.meta/.schemas/highlights.json"
title: "The `elasticsearch` sink `compression` option now defaults to `none`"
description: "Compression is not supported by all Elasticsearch service providers"
author_github: "https://github.com/binarylogic"
hide_on_release_notes: false
pr_numbers: [2219]
release: "0.9.0"
tags: ["type: breaking change","domain: sinks","sink: elasticsearch"]
---

To optimize throughput we originally defaulted the `elasticsearch` sink
`compression` option to `gzip`. It is our philosohpy that Vector's defaults
should optimize performance and throughput, but not at the expense of causing
errors. Unfortunately, AWS hosted Elasticsearch does not support compression,
and therefore we've made this feature opt-in.

### Upgrade Guide

Upgrading is easy. Add the following if you want to enabled Gzip compression:

```diff title="vector.toml"
 [sinks.es]
   type = "elasticsearch"
+  compression = "gzip"
```



