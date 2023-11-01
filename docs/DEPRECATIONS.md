See [DEPRECATION.md](docs/DEPRECATION.md#process) for the process for updating this file.

The format for each entry should be: `<version> <identifier> <description>`.

- `<version>` should be the version of Vector in which to take the action (deprecate, migrate, or
  remove)
- `<identifier>` should be a unique identifier that can also be used in the code to easily find the
  places to modify
- `<description>` should be a longer form description of the change to be made

## To be deprecated

## To be migrated

## To be removed

* Support for `v1` series endpoint in the `datadog_metrics` sink should be removed.
* legacy_openssl_provider v0.34.0 OpenSSL legacy provider flag should be removed
* armv7_rpm v0.34.0 The armv7 RPM packages should be removed (replaced by armv7hl)
