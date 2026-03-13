The `datadog_metrics` sink now defaults to the Datadog series v2 endpoint (`/api/v2/series`) and
exposes a new `series_api_version` configuration option (`v1` or `v2`) to control which endpoint is
used. Set `series_api_version: v1` to fall back to the legacy v1 endpoint if needed.

authors: vladimir-dd
