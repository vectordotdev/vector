The `internal_metrics` source has a new `emit_incremental` option that emits counters and histograms
as increments since the previous scrape instead of absolute totals. This keeps sinks like
`datadog_metrics` from dropping histograms observed within a single scrape interval and the first
scrape of each counter.

authors: gwenaskell
