---
date: "2025-05-05"
title: The Log Namespace feature
short: Log Namespace
description: Learn how the log namespacing works.
authors: ["pront"]
domain: schemas
weight: 2
tags: ["log_namespace", "logs", "namespace", "level up", "guides", "guide"]
---

{{< requirement >}}
Before you begin, this guide assumes the following:

* You understand that this feature, and the global schemas are mutually exclusive.
* When log namespacing is enabled, the [global schema settings] are ignored.
* This feature is still in `beta` so behavior might change.

[global schema settings]: /docs/reference/configuration/schema/#log_schema
[docs.setup.quickstart]: /docs/setup/quickstart/

If you encounter any issues please [report them here](https://github.com/vectordotdev/vector/issues/new?template=bug.yml).

{{< /requirement >}}

## Background

Vector traditionally stored metadata (like `host`, `timestamp`, and `source_type`) as top-level
fields alongside your log data. This "legacy" approach has a few drawbacks:

* **Field name collisions**: If your logs contain a field named `host`, it could conflict with
  Vector's metadata field
* **Unclear ownership**: It's not immediately obvious which fields are from your data and which
  are Vector metadata
* **Difficult transformations**: When you want to transform only your data (not metadata), you
  need to be careful to exclude metadata fields

The Vector namespace mode solves these issues by storing metadata in a separate namespace,
completely isolated from your log data.

## Default Behavior

### Vector Config

<details open>
  <summary>Show/Hide</summary>

```yaml
schema:
  log_namespace: true

sources:
  s0:
    type: demo_logs
    format: shuffle
    lines:
      - Hello World!
    interval: 10

sinks:
  text_console:
    type: console
    inputs:
    - s0
    encoding:
      codec: text

  json_console:
    type: console
    inputs:
    - s0
    encoding:
      codec: json
      json:
        pretty: true
```

</details>

### stdout

Sample output from `text_console`:

```text
Hello World!
```

Sample output from `json_console`:

```json
"Hello World!"
```

## Difference with global schema

If we switch this feature off:

```yaml
schema:
  log_namespace: false
```

Then we observe a big difference for these two encoders:

The `text` encoder only encodes the value that `log_schema.message_key` points to (which is `.message` by default).

Sample output from the `text_console` sink:

```text
{"host":"localhost","message":"Hello World!","service":"vector","source_type":"demo_logs","timestamp":"2025-05-01T19:06:12.227425Z"}
```

The `json` encoder passes the **whole** log to Serde JSON for encoding.

Sample output from the `json_console` sink:

```json
{
  "host": "localhost",
  "message": {
    "host": "localhost",
    "message": "Hello World!",
    "service": "vector",
    "source_type": "demo_logs",
    "timestamp": "2025-05-01T19:06:12.227425Z"
  },
  "service": "vector",
  "source_type": "demo_logs",
  "timestamp": "2025-05-01T19:06:12.227425Z"
}
```

The following example helps illustrate the difference between the two encoders.
Consider the input `"foo"`, which is a valid JSON string but not a JSON object.
Vector converts it into a structured object by wrapping it with the `log_schema.message_key` (e.g., `"message"`), resulting in: `{"message": "foo"}`.

{{< info >}}
We can always prepare events for ingestion by using a [remap](/docs/reference/configuration/transforms/remap/) transform and a suitable encoder in the sink.
{{< /info >}}

### Custom semantic meanings

#### Vector Config

<details open>
  <summary>Show/Hide</summary>

```yaml
schema:
  log_namespace: true

sources:
  s0:
    type: demo_logs
    format: shuffle
    lines:
      - Hello World!
    interval: 10

transforms:
  t0:
    type: remap
    inputs:
      - s0
    source: |
      set_semantic_meaning(.custom_field, "message")
      # This becomes the new payload. The `.` is overwritten.
      .custom_field = "foo"

  t1:
    type: remap
    inputs:
      - s0
    source: |
      # The value of `.` is `Hello World!` at this point, however the following line overwrites it.
      . = "bar"

sinks:
  text_console:
    type: console
    inputs:
    - t0
    encoding:
      codec: text

  json_console:
    type: console
    inputs:
    - t1
    encoding:
      codec: json
      json:
        pretty: true
```

</details>

#### stdout

Sample output from `text_console`:

```text
foo
```

Sample output from `json_console`:

```json
"bar"
```

## Migration Considerations

If you're considering migrating from legacy mode (`log_namespace = false`) to Vector namespace mode
(`log_namespace = true`), here are key things to be aware of:

### VRL Updates

VRL scripts that reference metadata fields will need to be updated to use the metadata accessor syntax:

**Legacy mode:**

```coffee
.host = "new-host"
.timestamp = now()
```

**Vector namespace mode:**

```coffee
%vector.host = "new-host"
%vector.ingest_timestamp = now()
```

### Sink Behavior Differences

Many sinks will behave differently depending on the namespace setting. Always test your sinks after switching modes to verify expected
behavior before deploying.

### Gradual Migration Strategy

You can configure `log_namespace` per-source if you need a gradual migration:

```yaml
# Global default (legacy)
schema:
  log_namespace: false

sources:
  # New source using Vector namespace
  new_source:
    type: http_server
    log_namespace: true

  # Existing source still using legacy
  existing_source:
    type: file
    # Uses global default (false)
```

This allows you to:

1. Keep existing pipelines working with legacy mode
2. Adopt Vector namespace mode for selected sources only
3. Migrate sources incrementally over time
