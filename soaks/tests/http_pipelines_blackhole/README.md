# HTTP -> Pipelines -> Blackhole

This soak tests the http sink feeding through multiple pipeline transforms down
to blackhole sink. It is a complicated topology.

## Method

Lading `http_gen` is used to generate log load into vector. The vector internal
blackhole is the sink for this configuration.
