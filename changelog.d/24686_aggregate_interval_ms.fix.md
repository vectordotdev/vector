The `aggregate` transform now correctly sets `interval_ms` on incremental counter metrics, allowing the Datadog metrics sink to encode them as rate metrics instead of count metrics.

authors: thomasqueirozb
