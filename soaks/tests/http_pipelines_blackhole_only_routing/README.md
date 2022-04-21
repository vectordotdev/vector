# HTTP -> Pipelines -> Blackhole

This soak tests the http sink feeding through multiple pipeline transforms down
to blackhole sink. It is a complicated topology. This relates to
`http_pipelines_blackhole` in that any remap step is nulled out in so far as is
possible to get a read on overhead.

## Method

Lading `http_gen` is used to generate log load into vector. The vector internal
blackhole is the sink for this configuration.
