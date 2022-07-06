# HTTP -> Pipelines (no grok parsing) -> Blackhole

This soak tests the http source feeding through multiple filter transforms
each containing a Datadog Search Syntax condition.

This is the same as the `http_pipelines_blackhole` soak, but with each
pipelines transform returning `true` and foregoing `parse_groks` processing.

## Method

Lading `http_gen` is used to generate log load into vector. The vector internal
blackhole is the sink for this configuration.
