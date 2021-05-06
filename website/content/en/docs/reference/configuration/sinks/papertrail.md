---
title: Papertrail
kind: sink
---

## Configuration

{{< component/config >}}

## Telemetry

{{< component/config >}}

## How it works

### Health checks

{{< snippet "health-checks" >}}

### Setup

1. Register for a free account at [Papertrailapp.com][app].
1. [Create a Log Destination][destination] and ensure that TCP is enabled.
1. Set the Log Destination as the [`endpoint`](#endpoint) option and start shipping your logs!

### State

{{< snippet "stateless" >}}

### Transport Layer Security (TLS)

{{< snippet "tls" >}}

[app]: https://papertrailapp.com/signup?plan=free
[destination]: https://papertrailapp.com/destinations/new
