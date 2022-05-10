# HTTP -> Filter (Datadog Search Syntax) -> Blackhole

This soak tests the http source filtering to a blackhole via Datadog Search
Syntax conditions. It is intended to match the HTTP -> Pipelines -> Blackhole
soak without any transformations, just filters.

Its intent is to measure the overhead of filtering and any changes to its
performance.

## Method

Lading `http_gen` is used to generate log load into vector. The vector internal
blackhole is the sink for this configuration.
