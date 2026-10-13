# InfluxDB sinks require an explicit version {#influxdb-version-required}

## Summary

The `version` option is now required for the `influxdb_logs` and `influxdb_metrics` sinks.
Omitting it was [deprecated in 0.58.0](https://vector.dev/deprecations/), and the version is no
longer inferred from the configured settings. Configurations without `version` now fail to load.

## Migration

Add `version: "1"` or `version: "2"` to match the configured InfluxDB settings. Use `"1"` with
`database`, `consistency`, `retention_policy_name`, `username`, and `password`; use `"2"` with
`org`, `bucket`, and `token`.

#### Old

```yaml
sinks:
  influx:
    type: influxdb_metrics
    endpoint: http://localhost:8086
    org: my-org
    bucket: my-bucket
    token: ${INFLUXDB_TOKEN}
```

#### New

```yaml
sinks:
  influx:
    type: influxdb_metrics
    endpoint: http://localhost:8086
    version: "2"
    org: my-org
    bucket: my-bucket
    token: ${INFLUXDB_TOKEN}
```

authors: JinchengCao
