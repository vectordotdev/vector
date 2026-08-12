---
what: "omitting the `version` field on `influxdb_logs` and `influxdb_metrics` sinks"
deprecated_since: "0.58.0"
---

The `version` field selects the InfluxDB API version whose settings are used. It will be required
in a future release. Until then, when `version` is unset the version is inferred from the
configured settings.

Migrate by adding `version: "1"` or `version: "2"` to match the settings already present:

```yaml
sinks:
  my_sink:
    type: influxdb_logs
    version: "2"
    org: my-org
    bucket: vector-bucket
    token: ${INFLUXDB_TOKEN}
```
