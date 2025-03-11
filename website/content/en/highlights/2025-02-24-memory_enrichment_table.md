---
date: "2025-02-24"
title: "Memory Enrichment Table"
description: "Introducing the memory enrichment table!"
authors: [ "pront" ]
pr_numbers: [ 21348 ]
release: "0.45.0"
hide_on_release_notes: false
badges:
  type: "new feature"
  domains: [ "enrichment", "sinks" ]
---

We are excited to announce the `memory` enrichment table!

Special thanks to [@esensar](https://github.com/esensar) for implementing this feature and to
[Quad9](https://quad9.net/) for sponsoring this work.

## Data Model

The memory table operates on logs and
accepts [VRL objects](/docs/reference/vrl/expressions/#object).
Each key-value pair is stored as a separate entry in the table, associating a value with its
corresponding key. Value here refers to VRL values.

## Building a cache

```yaml
enrichment_tables:
  memory_table:
    type: memory
    ttl: 60
    flush_interval: 5
    inputs: [ "cache_generator" ]

sources:
  demo_logs_test:
    type: "demo_logs"
    format: "json"

transforms:
  demo_logs_processor:
    type: "remap"
    inputs: [ "demo_logs_test" ]
    source: |
      . = parse_json!(.message)
      user_id = get!(., path: ["user-identifier"])

      # Check if we already have a cached value for this user in the enrichment table
      existing, err = get_enrichment_table_record("memory_table", { "key": user_id })

      if err == null {
        # A cached value exists; reuse it.
        # The `existing` object has this structure:
        # { "key": user_id, "value": {...}, "ttl": 50 }
        . = existing.value
        .source = "cache"
      } else {
        # No cached value found, process the event and prepare new data
        .referer = parse_url!(.referer)
        .referer.host = encode_punycode!(.referer.host)
        .source = "transform"
      }

  cache_generator:
    type: "remap"
    inputs: [ "demo_logs_processor" ]
    source: |
      # Check if this user is already in the cache
      key_value = get!(., path: ["user-identifier"])
      existing, err = get_enrichment_table_record("memory_table", { "key":  key_value })

      if err != null {
        # No cached value found, store the processed data in the enrichment table
        data = .

        # The memory enrichment table stores all key-value pairs it receives.
        # To structure it correctly, we create an object where:
        # - The key is the "user-identifier".
        # - The value is the rest of the processed event data.
        . = set!(value: {}, path: [get!(data, path: ["user-identifier"])], data: data)
      } else {
        # Already cached, do nothing
        . = {}
      }

# After some time, processed events will start having their "source" set to "cache",
# indicating that the data is being retrieved from the enrichment table.
sinks:
  console:
    inputs: [ "demo_logs_processor" ]
    target: "stdout"
    type: "console"
    encoding:
      codec: "json"
      json:
        pretty: true
```

You can imagine a real world scenario where the `demo_logs_processor` has to do some expensive
calculation the first time it encounters a key. Subsequently, every time the same key is encountered
the processing step will be skipped since the pre-computed value is present in the table. The values
can expire; in that case, the computation step will be repeated.

## Use as a sink

This new table type can also be used as a sink to feed it data, which can then be queried
like any other enrichment table. For example, here is how to introduce this new component as a sink
if you have another source that can populate the cache:

```yaml
  memory_table_sink:
    inputs: [ "another_source_or_transform" ]
    type: memory_enrichment_table
    ttl: 60
    flush_interval: 5
```

We plan to make this component even more flexible in the future. For example, it can also act as a
source. This exercise raises some important questions on component flexibility. The end goal is
treating components as nodes in a graph, unlocking even greater possibilities, such as chaining
sinks.
