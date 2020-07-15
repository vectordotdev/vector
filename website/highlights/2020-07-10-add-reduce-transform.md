---
last_modified_on: "2020-07-15"
$schema: "/.meta/.schemas/highlights.json"
title: "New Reduce transform"
description: "More flexibility around grouping events."
author_github: "https://github.com/hoverbear"
hide_on_release_notes: false
pr_numbers: [2870]
release: "0.10.0"
tags: ["type: new feature","domain: transforms"]
---

You can now find a new [Reduce][urls.vector_transform_reduce]! This allows you to turn a stream of many small events into a stream of less small events!

Say you had a stream of events like:

```json file=test.json
{ "transaction_id": 1, "sum_this": 1 }
{ "transaction_id": 2, "sum_this": 24 }
{ "transaction_id": 1, "stop_summing": true }
{ "transaction_id": 2, "sum_this": 24 }
{ "transaction_id": 3, "sum_this": 2 }
{ "transaction_id": 3, "sum_this": 2, "note": "This one will expire after a configured time" }
{ "transaction_id": 1, "sum_this": 1, "note": "This is a new one!" }
{ "transaction_id": 1, "stop_summing": true }
```

And you wanted to turn them into this:

```json file=output.json
{ "transaction_id": 1, "sum_this": 1 }
{ "transaction_id": 2, "sum_this": 48 }
{ "transaction_id": 1, "stop_summing": true }
{ "transaction_id": 2, "sum_this": 24 }
{ "transaction_id": 3, "sum_this": 2 }
{ "transaction_id": 3, "sum_this": 2, "note": "This one will expire after a configured time" }
{ "transaction_id": 1, "sum_this": 1, "note": "This is a new one!" }
{ "transaction_id": 1, "stop_summing": true }
```

You could use a transform like this!

```toml file=vector.toml
[transforms.my_transform_id]
  type = "reduce"
  inputs = ["database_log"]
  identifier_fields = ["transaction_id"]
  ends_when.type = "check_fields"
  ends_when."stop_summing.eq" = true
```

[urls.vector_transform_reduce]: https://vector.dev/docs/reference/transforms/reduce/
