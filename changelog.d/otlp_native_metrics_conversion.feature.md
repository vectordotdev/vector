Added native Vector metric to OTLP conversion support in the OTLP serializer. Vector metrics (Counter, Gauge, Histogram, Summary, Distribution, Set) are now automatically converted to the OTLP protobuf format when sent through OTLP sinks. Tags with `resource.*` and `scope.*` prefixes are reserved for OTLP structural mapping — they are decomposed back into OTLP resource attributes, instrumentation scope, and data point attributes. Native metrics from non-OTLP sources that coincidentally use these tag prefixes will have those tags routed into the OTLP resource/scope structures rather than remaining as flat data point attributes.

Sample configuration:

```yaml
sources:
  host_metrics:
    type: host_metrics
    collectors: [cpu, memory, disk]

  internal_metrics:
    type: internal_metrics

transforms:
  tag_resources:
    type: remap
    inputs: ["host_metrics", "internal_metrics"]
    source: |
      # Add resource-level tags (will become OTLP resource attributes)
      .tags."resource.service.name" = "vector"
      .tags."resource.host.name" = get_hostname!()
      # Add scope info
      .tags."scope.name" = "host-monitoring"
      .tags."scope.version" = "1.0.0"

sinks:
  otel_out:
    type: opentelemetry
    inputs: ["tag_resources"]
    endpoint: http://collector:4317
    encoding:
      codec: otlp  # Native metrics auto-converted to OTLP protobuf
```
