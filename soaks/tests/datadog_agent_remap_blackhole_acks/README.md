# Datadog Agent -> Remap -> Blackhole

This soak tests Datadog agent source feeding into the blackhole sink through a
non-trivial remap transform. It is a straight pipe otherwise.

This is the same soak test scenario as `datadog_agent_remap_blackhole`
but with end-to-end acknowledgements enabled. When end-to-end
acknowledgements become the default, these tests can be merged.

## Method

Lading `http_gen` is used to generate log load into vector. There is no sink
outside of vector.
