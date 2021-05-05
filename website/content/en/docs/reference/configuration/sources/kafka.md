---
title: Kafka
description: Collect logs from [Kafka](https://kafka.apache.org)
kind: source
---

## Configuration

{{< component/config >}}

## Output

{{< component/output >}}

## Telemetry

{{< component/telemetry >}}

## How it works

### Context

{{< snippet "context" >}}

### State

{{< snippet "stateless" >}}

### Transport Layer Security (TLS)

{{< snippet "tls" >}}

### librdkafka

The `kafka` sink uses [`librdkafka`][librdkafka] under the hood. This is a battle-tested, high-performance, and reliable library that facilitates communication with Kafka. As Vector produces static MUSL builds, this dependency is packaged with Vector, which means that you don't need to install it.

[librdkafka]: https://github.com/edenhill/librdkafka
