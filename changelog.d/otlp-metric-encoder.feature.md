The OTLP codec's serializer now supports native Vector `Metric` events for `Counter`, `Gauge`, `AggregatedHistogram`, and `AggregatedSummary` values, converting them into the OTLP `Sum`, `Gauge`, `Histogram`, and `Summary` protobuf types respectively. Previously, encoding a native metric event with the OTLP serializer always failed.

authors: petere-datadog
