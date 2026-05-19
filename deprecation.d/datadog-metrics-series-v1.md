---
what: "`series_api_version: v1` option on the `datadog_metrics` sink"
announcement_version: 0.55.0
deprecation_version: 0.58.0
---

The `series_api_version: v1` option is deprecated in favor of `v2` (the default).
The v1 series endpoint (`/api/v1/series`) is a legacy endpoint.

Users should remove `series_api_version: v1` from their configuration or set it to `v2`.
