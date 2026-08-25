The `influxdb_logs` and `influxdb_metrics` sinks now accept a `version` field to select the
InfluxDB API version whose settings are used. When unset, the version is inferred from the
configured settings, matching the previous behavior. The `version` field will be required in a
future release.

authors: thomasqueirozb
