---
date: '2025-09-23'
title: 'OTLP Support'
description: 'Introducing Opentelemetry Protocol support!'
authors: ['pront']
pr_numbers: [23524]
release: '0.50.0'
hide_on_release_notes: false
badges:
  type: 'new feature'
  domains: ['opentelemetry', 'otel', 'otlp']
---

## Summary

We are excited to announce that the `opentelemetry` source now supports
[OpenTelemetry protocol](https://opentelemetry.io/docs/specs/otel/protocol) decoding.

This now possible by using the `use_otlp_decoding` option. This setup allows shipping OTLP formatted logs to an OTEL collector without the
use of a `remap` transform. The same can be done for metrics and traces. However, OTLP formatted metrics cannot be converted to Vector's
metrics format. As a workaround, the OTLP metrics are converted to Vector log events while preserving the OTLP format. **This prohibits the use of metric
transforms like `aggregate` but it enables easy shipping to OTEL collectors.**

## Example Configuration 1

Here is an example on how to setup an OTEL -> Vector -> OTEL pipeline:

```yaml
sources:
  source0:
    type: opentelemetry
    grpc:
      address: 0.0.0.0:4317
    http:
      address: 0.0.0.0:4318
    use_otlp_decoding: true
sinks:
  otel_sink:
    inputs:
      - source0.logs
    type: opentelemetry
    protocol:
      type: http
      uri: http://otel-collector-sink:5318/v1/logs
      method: post
      encoding:
        codec: otlp
```

The above configuration will only work with Vector versions >= `0.51`.

## Example Configuration 2

Here is another pipeline configuration that can achieve the same as the above:

```yaml
otel_sink:
  inputs:
    - otel.logs
  type: opentelemetry
  protocol:
    type: http
    uri: http://localhost:5318/v1/logs
    method: post
    encoding:
      codec: protobuf
      protobuf:
        desc_file: path/to/opentelemetry-proto.desc
        message_type: opentelemetry.proto.collector.logs.v1.ExportLogsServiceRequest
        use_json_names: true
    framing:
      method: 'bytes'
    request:
      headers:
        content-type: 'application/x-protobuf'
```

The `desc` file was generated with the following command:

```bash
  protoc -I=/path/to/vector/lib/opentelemetry-proto/src/proto/opentelemetry-proto \\
    --include_imports \\
    --include_source_info \\
    --descriptor_set_out=opentelemetry-proto.desc \\
          $(find /path/to/vector/lib/opentelemetry-proto/src/proto/opentelemetry-proto -name '*.proto')
```
