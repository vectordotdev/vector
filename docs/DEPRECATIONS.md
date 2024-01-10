See [DEPRECATION.md](docs/DEPRECATION.md#process) for the process for updating this file.

The format for each entry should be: `<version> <identifier> <description>`.

- `<version>` should be the version of Vector in which to take the action (deprecate, migrate, or
  remove)
- `<identifier>` should be a unique identifier that can also be used in the code to easily find the
  places to modify
- `<description>` should be a longer form description of the change to be made

For example:

- v0.34.0 legacy_openssl_provider OpenSSL legacy provider flag should be removed

## To be deprecated

## To be migrated

- v0.37.0 strict_env_vars Change the default for missing environment variable interpolation from
  warning to erroring.

## To be removed
