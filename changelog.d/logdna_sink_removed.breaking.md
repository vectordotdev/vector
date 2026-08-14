# `logdna` sink alias removed {#logdna-sink-alias-removed}

## Summary

The deprecated `logdna` sink alias has been removed. It was renamed to `mezmo` in v0.29.0.
Configurations using `type: logdna` now fail validation.

## Migration

Rename the sink type from `logdna` to `mezmo`:

#### Old

```yaml
sinks:
  my_sink_id:
    type: logdna
    api_key: ${LOGDNA_API_KEY}
    hostname: ${HOSTNAME}
```

#### New

```yaml
sinks:
  my_sink_id:
    type: mezmo
    api_key: ${LOGDNA_API_KEY}
    hostname: ${HOSTNAME}
```

authors: thomasqueirozb
