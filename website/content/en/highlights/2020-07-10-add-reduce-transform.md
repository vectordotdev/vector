---
date: "2020-07-17"
title: "New Reduce transform"
description: "Canonical Log Lines in Vector"
authors: ["hoverbear"]
hide_on_release_notes: false
pr_numbers: [2870]
release: "0.10.0"
badges:
  type: "new feature"
  domains: ["transforms"]
---

Fan of [Stripe's Canonical Log Lines][urls.stripe_blog_canonical_log_lines]? We are too. You can now find a new [Reduce][docs.transforms.reduce]! This allows you to turn a stream of many small events into a stream of less small events!

The fantastic article by Brandur Leach describes them best:

> They’re a simple idea: in addition to their normal log traces, requests (or some other unit of work that’s executing)
> also emit one long log line at the end that pulls all its key telemetry into one place.

Here's a _canonical_ example:

```log
[2019-03-18 22:48:32.999] canonical-log-line alloc_count=9123 auth_type=api_key database_queries=34 duration=0.009 http_method=POST http_path=/v1/charges http_status=200 key_id=mk_123 permissions_used=account_write rate_allowed=true rate_quota=100 rate_remaining=99 request_id=req_123 team=acquiring user_id=usr_123
```

Let's build similar in Vector!

We'll take a series of events:

```log title=input.log
{"timestamp": "...", "message": "Received GET /path", "request_id": "abcd1234", "request_path": "/path", "request_params": "..."}
{"timestamp": "...", "message": "Executed query in 5.2ms", "request_id": "abcd1234", "query": "SELECT * FROM table", "query_duration_ms": 5.2}
{"timestamp": "...", "message": "Rendered partial _partial.erb in 2.3ms", "request_id": "abcd1234", "template": "_partial.erb", "render_duration_ms": 2.3}
{"timestamp": "...", "message": "Executed query in 7.8ms", "request_id": "abcd1234", "query": "SELECT * FROM table", "query_duration_ms": 7.8}
{"timestamp": "...", "message": "Sent 200 in 15.2ms", "request_id": "abcd1234", "response_status": 200, "response_duration_ms": 5.2}
```

Then output this (but not formatted so nicely!):

```json title=output.log
{
  "timestamp_start": "...",
  "timestamp_end": "...",
  "request_id": "abcd1234",
  "request_path": "/path",
  "request_params": "...",
  "query_duration_ms": 13.0,
  "render_duration_ms": 2.3,
  "status": 200,
  "response_duration_ms": 5.2
}
```

We'll run this config:

```toml title=vector.toml
data_dir = "tmp"

[sources.source0]
  include = ["input.log"]
  start_at_beginning = true
  type = "file"
  fingerprinting.strategy = "device_and_inode"

[transforms.transform0]
  inputs = ["source0"]
  type = "json_parser"
  field = "message"

[transforms.transform1]
  inputs = ["transform0"]
  type = "reduce"
  identifier_fields = ["request_id"]
  ends_when.type = "check_fields"
  ends_when."response_status.exists" = true
  merge_strategies.message = "discard"
  merge_strategies.query = "discard"
  merge_strategies.template = "discard"
  merge_strategies.query_duration_ms = "sum"
  merge_strategies.render_duration_ms = "sum"
  merge_strategies.response_duration_ms = "sum"

[sinks.sink0]
  healthcheck = true
  inputs = ["transform1"]
  type = "file"
  path = "output.log"
  encoding = "ndjson"
  buffer.type = "memory"
  buffer.max_events = 500
  buffer.when_full = "block"s
```

We hope you find this useful!

[docs.transforms.reduce]: /docs/reference/configuration/transforms/reduce/
[urls.stripe_blog_canonical_log_lines]: https://stripe.com/blog/canonical-log-lines
