---
title: Vector
kind: source
---

## Requirements

{{< component/requirements >}}

## Configuration

{{< component/config >}}

## Output

{{< component/output >}}

## Telemetry

{{< component/telemetry >}}

## How it works

### Communication protocol

Upstream Vector instances forward data to downstream Vector instances via the TCP protocol.

### Encoding

Data is encoded via Vector's [event protobuf][event_proto] before it is sent over the wire.

### Message acknowledgment

{{< snippet "ack" >}}

### State

{{< snippet "stateless" >}}

### Transport Layer Security (TLS)

{{< snippet "tls" >}}

[event_proto]: https://github.com/timberio/vector/blob/master/proto/event.proto
