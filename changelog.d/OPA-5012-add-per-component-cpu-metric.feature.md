Added a new counter metric `component_cpu_usage_ns_total` counting the CPU
time consumed by a transform in nanoseconds.

The metric is opt-in: set `measure_cpu_usage: true` on individual transform
configurations to enable it. When disabled (the default), no counter is
registered and no per-poll clock sampling takes place.

authors: gwenaskell
