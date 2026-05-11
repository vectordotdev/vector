The `databricks_zerobus` sink no longer exits Vector at startup when the Unity Catalog table is missing or OAuth credentials are invalid. Schema resolution and stream creation are now deferred to the healthcheck (and to the first batch when `healthcheck.enabled = false`), so failures surface per-batch via the existing retry/event-status path instead of as a fatal config error. Process exit on a misconfigured target is now opt-in via `require_healthy: true`, matching the behavior of `aws_s3` and `kafka`.

authors: flaviocruz
