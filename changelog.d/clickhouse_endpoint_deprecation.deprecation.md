The `clickhouse` sink's `endpoint` configuration option has been deprecated in favor of the new `endpoints` option. While `endpoint` will continue to work for now, it is recommended to migrate to `endpoints` which supports multiple ClickHouse instances for high availability.

To migrate, change your configuration from:
```toml
endpoint = "http://localhost:8123"
```
to:
```toml
endpoints = ["http://localhost:8123"]
```

The `endpoint` option will be removed in a future release.

authors: pinylin
