---
title: Log events
weight: 1
tags: ["logs", "events", "schema"]
---

{{< svg "img/data-model-log.svg" >}}

A **log event** in Vector is a structured representation of a point-in-time event. It contains an arbitrary set of fields that describe the event.

A key tenet of Vector is **schema neutrality**. This ensures that Vector can work with any schema, supporting legacy and future schemas as your needs evolve. Vector doesn't require *any* specific fields and each [component][components] documents the fields it provides.

Here's an example representation of a log event (as JSON):

```json
{
  "log": {
    "custom": "field",
    "host": "my.host.com",
    "message": "Hello world",
    "timestamp": "2020-11-01T21:15:47+00:00"
  }
}
```

## Schema

{{< config/log-schema >}}

## How it works

### Schemas

{{< snippet "how-it-works/schemas" >}}

### Types

{{< snippet "how-it-works/types" >}}

[components]: /components
