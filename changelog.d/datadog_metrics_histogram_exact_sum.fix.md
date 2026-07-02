The `datadog_metrics` sink now preserves the exact `sum` (and the average derived from it) carried by aggregated histograms when converting them to Datadog sketches. Previously both were re-approximated by uniformly interpolating each bucket's count across the bucket's bounds, so the reported `sum`/`avg` of histograms ingested via e.g. the `opentelemetry` or `prometheus_scrape` sources drifted from the true values by up to the relative bucket width — orders of magnitude for values far smaller than the first bucket boundary.

authors: Ziv-Wenrix
