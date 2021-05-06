---
title: Vector
kind: sink
---

## Configuration

{{< component/config >}}

## Telemetry

{{< component/config >}}

## How it works

### Communication protocol

Upstream Vector instances forward data to downstream Vector instances via the TCP protocol.

### Context

{{< snippet "context" >}}

### Encoding

Data is encoded via Vector's [event protobuf][event_proto] before it is sent over the wire.

### Health checks

{{< snippet "health-checks" >}}

### Message acknowledgment

{{< snippet "ack" >}}

### State

{{< snippet "stateless" >}}

### Transport Layer Security (TLS)

{{< snippet "tls" >}}

[event_proto]: https://github.com/timberio/vector/blob/master/lib/vector-core/proto/event.proto
