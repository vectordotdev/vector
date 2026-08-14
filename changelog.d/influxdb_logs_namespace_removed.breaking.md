# `influxdb_logs` sink `namespace` option removed {#influxdb-logs-namespace-removed}

## Summary

The deprecated `namespace` option has been removed from the `influxdb_logs` sink. It has been
deprecated since v0.24.0 in favor of `measurement`. Configurations using it now fail validation.

## Migration

Replace `namespace` with `measurement`. Previously, `namespace` prefixed the measurement name
with `<namespace>.vector`, so set `measurement` to `<namespace>.vector` for the same effect:

#### Old

```yaml
sinks:
  my_sink_id:
    type: influxdb_logs
    namespace: my-namespace
    endpoint: http://localhost:8086
```

#### New

```yaml
sinks:
  my_sink_id:
    type: influxdb_logs
    measurement: my-namespace.vector
    endpoint: http://localhost:8086
```

Note: if your configuration sets both `namespace` and `measurement`, `measurement` was used and
`namespace` ignored; remove the `namespace` option and leave `measurement` unchanged.

authors: thomasqueirozb
