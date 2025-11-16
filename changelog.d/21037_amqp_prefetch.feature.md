---
title: "Add `prefetch_count` option to AMQP source"
description: |
  Introduces a new optional configuration field `prefetch_count` for the `amqp` source.
  This option defines the maximum number of unacknowledged messages per consumer
  by applying AMQP QoS (`basic.qos`) before consumption starts.

  When unset, Vector does not explicitly configure QoS and the broker/client defaults are used.
type: "enhancement"
component: "amqp source"
issue: "21037"
change_category: "user-facing"
changelog_entry_kind: "feature"
---

### Added
- Added a new `prefetch_count` option to the AMQP source configuration.
  This allows limiting the number of in-flight (unacknowledged) messages
  per consumer using RabbitMQ's prefetch mechanism (`basic.qos`).
  Setting this value helps control memory usage and load when processing messages slowly.

### Example
```yaml
sources:
  rabbitmq_source:
    type: amqp
    connection_string: "amqp://guest:guest@localhost:5672/%2f"
    queue: logs_q
    consumer: vector-consumer
    acknowledgements: true
    prefetch_count: 2
    decoding:
      codec: json


authors: elkh510
