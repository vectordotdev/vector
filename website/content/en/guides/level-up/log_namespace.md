---
title: The Log Namespace feature
short: Log Namespace
description: Learn how the log namespacing works.
author_github: https://github.com/pront
domain: schemas
weight: 2
tags: ["log_namespace", "logs", "namespace", "level up", "guides", "guide"]
---

{{< requirement >}}
Before you begin, this guide assumes the following:

* You understand that this feature, and the global schemas are mutually exclusive.
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
      .message = .
      .from = "t0"

  t1:
    type: remap
    inputs:
      - s0
    source: |
      .message = .
      .from = "t1"

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
{"from":"t0","message":"Hello World!"}
```

Sample output from `json_console`:

```json
{
  "from": "t1",
  "message": "Hello World!"
}
```

## Difference with global schema

If we switch this feature off:

```yaml
schema:
  log_namespace: false
```

Then we observe a big difference for these two encoders:

* The `text` encoder only encodes the value that `log_schema.message_key` points to (which is `.message` by default).
* The `json` encoder passes the **whole** log to Serde JSON for encoding.
  * For example `"foo"` is a valid JSON string but not a JSON object.
  * We convert it to a JSON object with the meaning as the key e.g. `{"message": "foo" }`.

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
      # Preserve the `Hello World!` payload.
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
