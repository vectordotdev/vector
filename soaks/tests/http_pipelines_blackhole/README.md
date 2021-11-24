# HTTP -> Pipelines -> Datadog Logs

This soak tests the http sink feeding through multiple pipeline transforms down
to blackhole sink. It is a complicated topology.

## Method

Lading `http_gen` is used to generate log load into vector. The vector internal
blackhole is sink for this configuration.
