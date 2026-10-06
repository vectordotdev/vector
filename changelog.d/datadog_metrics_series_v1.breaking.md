# Datadog metrics series v1 removed {#datadog-metrics-series-v1}

## Summary

The `datadog_metrics` sink no longer accepts `series_api_version: v1`, which was
[deprecated in 0.56.0](https://vector.dev/deprecations/).
Configurations that select `v1` now fail to load.

## Migration

Remove `series_api_version: v1` to use the default `v3` endpoint, or set
`series_api_version: v2` if your destination requires the v2 endpoint.

authors: pront
