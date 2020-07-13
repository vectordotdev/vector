---
last_modified_on: "2020-07-13"
$schema: "/.meta/.schemas/highlights.json"
title: "Batching and buffering reworked"
description: "Vector's batching and buffering matures."
author_github: "https://github.com/hoverbear"
hide_on_release_notes: false
pr_numbers: [2866]
release: "0.10.0"
tags: ["type: enhancement", "domain: sinks"]
---

We upgraded our sink batching/buffering design to better handle `max_events` and `max_bytes` style options. You can find the [reference documentation][urls.vector_sink_http_batch] sinks is updated.

```diff title="vector.toml"
  [sinks.my_sink_id]
    type = "http" # required
    inputs = ["my-source-or-transform-id"] # required
    uri = "https://10.22.212.22:9000/endpoint" # required
+   batch.max_bytes = 1049000 # optional, default, bytes
+   batch.max_events = 1000 # optional, no default, events
+   batch.timeout_secs = 1 # optional, default, seconds
```

As a result of this change, you may discover new options in the sinks you're using.

[urls.vector_sink_http_batch]: https://vector.dev/docs/reference/sinks/http/#batch
