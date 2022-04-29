# HTTP -> Pipelines -> Blackhole

This soak tests the http sink feeding through multiple pipeline transforms down
to blackhole sink. It is a complicated topology.

This is the same soak test scenario as `http_pipelines_blackhole`
but with end-to-end acknowledgements enabled. When end-to-end
acknowledgements become the default, these tests can be merged.

## Method

Lading `http_gen` is used to generate log load into vector. The vector internal
blackhole is sink for this configuration.
