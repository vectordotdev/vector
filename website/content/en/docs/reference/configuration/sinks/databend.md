---
title: Databend
description: Deliver log data to the [Databend](https://databend.rs) database
component_kind: sink
layout: component
tags: ["datafuselabs", "databend", "component", "sink", "storage", "logs"]
---

{{/*
This doc is generated using:

1. The template in layouts/docs/component.html
2. The relevant CUE data in cue/reference/components/...
*/}}

## Raw ingest and replace mode

The Databend sink supports staged batch loading by default. Set `load_mode` to
`streaming` to use Databend's streaming load API for normal inserts.

For staged loads, `copy_options.on_error` controls Databend COPY error handling.
It defaults to `abort`. Set it to `continue` to skip bad rows and continue
loading the rest of the staged file:

```yaml
sinks:
  databend:
    type: databend
    inputs: ["logs"]
    endpoint: "databend://root:@127.0.0.1:8000/default?sslmode=disable"
    table: "events"
    copy_options:
      on_error: continue
```

Set `primary_key` to use `REPLACE INTO ... ON (...)` with staged loading. When
`primary_key` is empty, the sink uses normal insert mode.

```yaml
sinks:
  databend:
    type: databend
    inputs: ["logs"]
    endpoint: "databend://root:@127.0.0.1:8000/default?sslmode=disable"
    table: "events"
    primary_key: ["id", "source"]
```

`primary_key` is independent of raw mode. Databend does not currently support
replace with streaming load, so `primary_key` cannot be used with
`load_mode: streaming`.

Raw mode writes each event into a generated raw ingest schema. The sink always
uses these columns:

```sql
raw_data JSON,
add_time TIMESTAMP
```

`raw.metadata.includes` adds metadata columns to that schema. Metadata paths are
converted to column names by removing `%` and replacing separators with `_`. For
example, `%kafka.topic` becomes `kafka_topic`.

Enable `raw.create_table` to create this table during sink startup. With the
following configuration, the generated table includes `raw_data`, `add_time`,
`kafka_topic`, `kafka_partition`, and `kafka_offset`:

```yaml
sinks:
  databend:
    type: databend
    inputs: ["logs"]
    endpoint: "databend://root:@127.0.0.1:8000/default?sslmode=disable"
    table: "raw_events"
    raw:
      enabled: true
      create_table: true
      metadata:
        includes:
          - "%kafka.topic"
          - "%kafka.partition"
          - "%kafka.offset"
```

`raw.metadata.includes` accepts Vector metadata paths. If the option is not
configured, it defaults to `["*"]`, which copies all metadata into a `metadata`
JSON column. Set it to an empty array to omit metadata columns:

```yaml
raw:
  metadata:
    includes: []
```

You can include specific metadata paths as separate columns:

```yaml
raw:
  metadata:
    includes:
      - "%kafka.topic"
      - "%kafka.partition"
      - "%kafka.offset"
```
