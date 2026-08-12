# InfluxDB sinks will require a `version` field {#influxdb-version-field}

## Summary

The `influxdb_logs` and `influxdb_metrics` sinks now accept an optional `version` field to select
the InfluxDB API version whose settings are used. When unset, the version is inferred from the
configured settings (matching the previous behavior). The `version` field will be required in a
future release.

## Migration

Add `version: "1"` or `version: "2"` to `influxdb_logs` and `influxdb_metrics` configs, matching
the settings already present. Configs that omit `version` keep working but emit a deprecation
warning.

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
