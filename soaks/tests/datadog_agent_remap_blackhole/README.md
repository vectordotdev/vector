# Datadog Agent -> Remap -> Blackhole

This soak tests Datadog agent source feeding into the blackhole sink through a
non-trivial remap transform. It is a straight pipe otherwise.

## Method

Lading `http_gen` is used to generate log load into vector. There is no sink
outside of vector.
