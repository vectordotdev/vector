---
last_modified_on: "2020-04-13"
$schema: "/.meta/.schemas/highlights.json"
title: "Sink options have been refactored"
description: "<fill-in>"
author_github: "https://github.com/binarylogic"
pr_numbers: [1006, 1493, 1494, 1495]
release: "0.7.0"
importance: "low"
tags: ["type: breaking change", "domain: buffers", "domain: config", "domain: sinks"]
---

Request based sinks have had their request fields nested under the table
`request` and no longer use fixed retry intervals, instead using a fibonacci
sequence for backing off retry attempts.

Batching fields are now nested under the table `batch`, with the field `size`
replaced with either `max_events` or `max_size` in order to clarify its purpose
(capping discrete events or bytes respectively).

Finally, authentication fields have been moved from the table `basic_auth` into
a general purpose `auth` table complemented with a `strategy` field.

These changes effect the following sinks:

* `aws_cloudwatch_logs`
* `aws_kinesis_firehose`
* `aws_kinesis_streams`
* `aws_s3`
* `clickhouse`
* `datadog_metrics`
* `elasticsearch`
* `gcp_pubsub`
* `http`
* `new_relic_logs`
* `splunk_hec`

In order to migrate all fields prefixed with `request_` must be placed within a
`request` table with the prefix removed.

The config field `retry_backoff_secs` must also be replaced with two new fields
`retry_initial_backoff_secs` and `retry_max_duration_secs`.

Fields prefixed with `batch_` must be placed within a `batch` table with the
prefix removed. Instances of `batch_size` should be renamed `max_size` or
`max_events` (refer to the relevant sink docs for the correct variant).

Finally, the table `basic_auth` should renamed `auth` with a field `strategy`
added:

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



