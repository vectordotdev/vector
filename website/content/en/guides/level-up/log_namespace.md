---
title: The Log Namespace feature
short: Log Namespace
description: Learn how the log namespacing works.
author_github: https://github.com/hoverbear
domain: schemas
weight: 2
tags: ["log_namespace", "logs", "namespace", "level up", "guides", "guide"]
---

{{< requirement >}}
Before you begin, this guide assumes the following:

* You understand this feature and the global schema are mutually exclusive. 
* When log namespacing is enabled, the [global schema settings] are ignored.
* This feature is still in `beta` so behavior might change.

[global schema settings]: /docs/reference/configuration/global-options/#log_schema
[docs.setup.quickstart]: /docs/setup/quickstart/
{{< /requirement >}}

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

transforms:
  t0:
    type: remap
    inputs:
      - s0
    source: |
      %from = "t0"
      .message = .
      .meta = %

  t1:
    type: remap
    inputs:
      - s0
    source: |
      .message = "overwrite"
      %from = "t1"
      .meta = %

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

### stdout
Sample output from `text_console`:

```text
{"message":"Hello World!","meta":{"demo_logs":{"host":"localhost","service":"vector"},"from":"t0","vector":{"ingest_timestamp":"2025-04-23T17:42:39.568583Z","source_type":"demo_logs"}}}
```

Sample output from `json_console`:

```json
{
  "message": "overwrite",
  "meta": {
    "demo_logs": {
      "host": "localhost",
      "service": "vector"
    },
    "from": "t1",
    "vector": {
      "ingest_timestamp": "2025-04-23T17:42:39.568583Z",
      "source_type": "demo_logs"
    }
  }
}
```

## Difference with global schema

If we switch this feature off:

```yaml
schema:
  log_namespace: false
```

Then we will observe a big difference for these two encoders:
- The `text` encoder only encodes the result of `log.get_message()`  
- The `json` encoder passes the **whole** log to serde JSON for encoding.
  - For example `"foo"` is a valid JSON string but not a JSON object.
  - We convert it to a JSON object with the meaning as the key e.g. `{"message": "foo" }`.
 
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
      .custom_field = "foo"

  t1:
    type: remap
    inputs:
      - s0
    source: |
      .message = .
      .custom_field = "bar"

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
{
  "custom_field": "bar",
  "message": "Hello World!"
}
```
