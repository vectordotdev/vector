---
title: HTTP
description: Receive logs via [HTTP](https://en.wikipedia.org/wiki/Hypertext_Transfer_Protocol#Client_request)
kind: source
---

## Configuration

{{< component/config >}}

## Output

{{< component/output >}}

## Telemetry

{{< component/telemetry >}}

## Examples

{{< component/examples >}}

## How it works

### Context

{{< snippet "context" >}}

### Decompression

The received body is decompressed according to the `Content-Encoding` header. Supported algorithms are `gzip`, `deflate`, and `snappy`.

### State

{{< snippet "stateless" >}}

### Transport Layer Security (TLS)

{{< snippet "tls" >}}
