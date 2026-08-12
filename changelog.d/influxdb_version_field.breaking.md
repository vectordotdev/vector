# InfluxDB sinks require a `version` field {#influxdb-version-field}

## Summary

The `influxdb_logs` and `influxdb_metrics` sinks now require a `version` field to select the
InfluxDB API version whose settings are used. Previously the version was inferred from which
settings were present, which made it possible to configure both versions at once and fail at
build time with an unclear error.

## Migration

Add `version: "1"` or `version: "2"` to existing `influxdb_logs` and `influxdb_metrics` configs,
matching the settings already present.

#### Old

```yaml
sinks:
  my_sink:
    type: influxdb_logs
    org: my-org
    bucket: vector-bucket
    token: ${INFLUXDB_TOKEN}
```

#### New

```yaml
sinks:
  my_sink:
    type: influxdb_logs
    version: "2"
    org: my-org
    bucket: vector-bucket
    token: ${INFLUXDB_TOKEN}
```

authors: thomasqueirozb
