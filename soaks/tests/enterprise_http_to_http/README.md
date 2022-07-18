# Enterprise HTTP -> HTTP

This enterprise-related soak test tracks the throughput overhead of enterprise
reporting components: `internal_metrics`, `internal_logs`, `datadog_metrics`,
`datadog_logs`, etc. The user-configured Vector topology is simply an `http`
source feeding directly in to an `http` sink.

## Method

Lading `http_gen` is used to generate log load into vector, `http_blackhole`
acts as a HTTP sink.
