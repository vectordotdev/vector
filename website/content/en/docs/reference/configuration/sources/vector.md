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

Currently, Vector doesn't perform any application-level message acknowledgement. This means that individual messages can be lost, although this should be rare.

### State

{{< snippet "stateless" >}}

### Transport Layer Security (TLS)

{{< snippet "tls" >}}

[event_proto]: https://github.com/timberio/vector/blob/master/proto/event.proto
