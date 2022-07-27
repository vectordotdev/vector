# Enterprise HTTP -> HTTP

This enterprise-related soak test tracks the throughput overhead of enterprise
reporting components: `host_metrics`, `internal_metrics`, `internal_logs`,
`datadog_metrics`, `datadog_logs`, various `remap`s, etc. The user-configured
Vector topology is simply an `http` source feeding directly in to an `http`
sink.

Note that this Vector configuration does not actually report any enterprise
information to Datadog. Instead, we mock the Datadog reporting endpoints using
Lading's `http_blackhole`. While we sacrifice a bit of realism, we avoid having
to setup CI secrets, complicate the soak test scripts with environment variable
passing, and handle flaky results/issues using real Datadog endpoints. This soak
can be considered a best-case scenario wherein Datadog responds quickly and
always with success.

## Method

Lading `http_gen` is used to generate log load into vector, `http_blackhole`
acts as a HTTP sink.
