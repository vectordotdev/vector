# HTTP -> Filter (Datadog Search Syntax) -> Blackhole

This soak tests the http source filtering to a blackhole via Datadog Search
Syntax conditions.

## Method

Lading `http_gen` is used to generate log load into vector. The vector internal
blackhole is the sink for this configuration.
