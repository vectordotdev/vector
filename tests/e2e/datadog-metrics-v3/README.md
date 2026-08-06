This e2e test covers the `datadog_metrics` sink's V3 columnar protobuf
format, for both the series and sketches endpoints.

It reuses the `datadog_agent` source + DogStatsD emitter topology from
the [`datadog-metrics`](../datadog-metrics/README.md) V1/V2 suite, with
one addition: `data/agent.yaml` sets `use_v3_api.series.endpoints` to
force the Agent to send V3 series to `fakeintake-agent` (`dd_url`)
while forcing V2 to `vector` (`additional_endpoints`) — the only
series format Vector's `datadog_agent` source can ingest. Vector then
re-encodes what it ingested to V3 on its way to `fakeintake-vector`.
That gives a real Agent-native V3 series baseline to diff against,
the same "single shared Agent" trick the V1/V2 suite uses.

The test in [`tests/e2e/datadog/metrics/v3.rs`](../datadog/metrics/v3.rs)
lives as a sibling of the V1/V2 suite's `v1v2` module (which holds
`series`/`sketches`) under `datadog/metrics/`, and reuses `v1v2::series`'s
comparison helpers (`SeriesContext`/`TimeBucket`/`generate_series_intake`/
`common_series_assertions`/`compare_intakes`) rather than reimplementing
series comparison from scratch — it just converts fakeintake's decoded
V3 JSON into the same `MetricSeries`/`MetricPayload` those helpers
already operate on. `v1v2` and `v3` are siblings rather than one
nested inside the other so each suite's e2e `test_filter` (a bare
substring match) can target its own tests exactly without also
matching the other's.

Sketches have no such baseline: dogstatsd histograms/distributions
still go out via the Agent's old `/api/beta/sketches` route regardless
of `use_v3_api`, so there's no Agent-native V3 sketch payload to diff
against. fakeintake also has no V3 sketches decoder at all (unlike V3
series, which it decodes for us via `?format=json`). So sketches are
decoded locally via a minimal columnar-format reader and checked
structurally only — tag/resource dictionaries and sketch bins are
intentionally not reconstructed. See the module doc comment for the
full rationale on both points.
