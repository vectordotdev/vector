See [DEPRECATION.md](docs/DEPRECATION.md#process) for the process for updating this file.

The format for each entry should be: `<version> <identifier> <description>`.

- `<version>` should be the version of Vector in which to take the action (deprecate, migrate, or
  remove)
- `<identifier>` should be a unique identifier that can also be used in the code to easily find the
  places to modify
- `<description>` should be a longer form description of the change to be made

For example:

- legacy_openssl_provider v0.34.0 OpenSSL legacy provider flag should be removed

## To be deprecated

## To be migrated

- file_metric_tag v0.36.0 The `internal_metrics.include_file_tag` should default to `false`.

## To be removed

- datadog_v1_metrics v0.35.0 Support for `v1` series endpoint in the `datadog_metrics` sink should be removed.
- http_internal_metrics v0.35.0 `requests_completed_total`, `request_duration_seconds`, and `requests_received_total` internal metrics should be removed.
