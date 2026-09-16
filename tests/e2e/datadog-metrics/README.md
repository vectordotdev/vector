This e2e test covers the `datadog_agent` source, and the
`datadog_metrics` sink.

An emitter compose service runs a python DogStatsD program,
to generate various metric types for the test cases.

A single Agent container receives the metrics and forwards them to two
destinations simultaneously via `dd_url` and `additional_endpoints`:

1. `fakeintake-agent` directly — the baseline.
2. `vector`, which then forwards to `fakeintake-vector` — the compare case.

Using a single agent ensures both paths receive the same computed metric
values from the same flush window, making histogram statistics (avg, median,
percentiles) directly comparable. Previously, two independent agents could
compute different histogram values due to offset flush boundaries.

The two sets of data should be shaped the same in terms of when
the events were received, and the content of the events, but the
timestamps themselves are not guaranteed to align.
