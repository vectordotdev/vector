---
date: "2020-07-13"
title: "Sink options have been refactored"
description: "We've simplified and organized our sink options"
authors: ["binarylogic"]
pr_numbers: [1006, 1493, 1494, 1495]
release: "0.7.0"
hide_on_release_notes: false
badges:
  type: "breaking change"
  domains: ["buffers", "config", "sinks"]
---

In our preparation for 1.0 we took time to organize and cleanup our
request-based sink options. The specific changes include:

1. `request_*` options have been moved under a `request` table.
2. `retry_backoff_secs` must also be replaced with two new fields
   `retry_initial_backoff_secs` and `retry_max_duration_secs`.
3. `batch_*` options have been moved under a `batch` table.
4. `batch_size` has been replaced with either `batch.max_events` or
   `batch.max_size` in order to clarify its purpose (capping discrete events or
   bytes respectively).
5. `basic_auth` fields have been moved to a general purpose `auth` table
   complemented with a `strategy` field.

These changes effect the following sinks:

- `aws_cloudwatch_logs`
- `aws_kinesis_firehose`
- `aws_kinesis_streams`
- `aws_s3`
- `clickhouse`
- `datadog_metrics`
- `elasticsearch`
- `gcp_pubsub`
- `http`
- `new_relic_logs`
- `splunk_hec`

## Upgrade Guide

```diff title="vector.toml"
 [sinks.my_sink_id]
   type = "http"
   inputs = ["my-source-id"]
   uri = "https://10.22.212.22:9000/endpoint"

-  batch_size = 1049000
+  [sinks.my_sink_id.batch]
+    max_size = 1049000

-  [sinks.my_sink_id.basic_auth]
+  [sinks.my_sink_id.auth]
+    strategy = "basic"
     user = "${USERNAME_ENV_VAR}"
     password = "${PASSWORD_ENV_VAR}"

-  request_in_flight_limit = 5
-  request_retry_backoff_secs = 1
+  [sinks.my_sink_id.request]
+    in_flight_limit = 5
+    retry_initial_backoff_secs = 1
+    retry_max_duration_secs = 10
```
