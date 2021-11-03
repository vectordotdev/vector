---
date: "2020-07-13"
title: "Use external tagging for metrics serialization"
description: "We've improved the way we serialize metrics data"
authors: ["binarylogic"]
hide_on_release_notes: true
pr_numbers: [2231]
release: "0.9.0"
badges:
  type: "breaking change"
---

We've improved the serialized structure of our metrics events. This change
should only affect a very small, and rare, number of users. If you are consuming
metrics data from Vector's `console` sink then you'll need to adjust any
downstream systems to work with the new structure.

For example, previously a counter was serialized like:

```json
{
  "name": "login.count",
  "timestamp": "2019-11-01T21:15:47+00:00",
  "kind": "absolute",
  "tags": {
    "host": "my.host.com"
  },
  "value": {
    "type": "counter", // <-- metric type
    "value": 24.2
  }
}
```

It now serialized like:

```json
{
  "name": "login.count",
  "timestamp": "2019-11-01T21:15:47+00:00",
  "kind": "absolute",
  "tags": {
    "host": "my.host.com"
  },
  "counter": {
    // <-- metric type
    "value": 24.2
  }
}
```

## Upgrade Guide

Upgrading should involve handling changes in any systems that are consuming
metrics data from the `console` sink.
