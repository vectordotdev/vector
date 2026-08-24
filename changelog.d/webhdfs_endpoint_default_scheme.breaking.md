# `webhdfs` sink defaults endpoints to `https://` {#webhdfs-sink-defaults-endpoints-to-https}

## Summary

The `webhdfs` sink's `endpoint` option now defaults a missing scheme to `https://` instead of
`http://`. A scheme-less endpoint (for example `endpoint: "127.0.0.1:9870"`) still loads and
remains valid, but it now resolves to `https://127.0.0.1:9870`; previously the underlying
WebHDFS client resolved to `http://127.0.0.1:9870`

## Migration

Add an explicit scheme to the `endpoint` value. Use `http://` for a plain-HTTP server and
`https://` for a TLS-enabled one.

#### Old

```yaml
sinks:
  hdfs:
    type: webhdfs
    endpoint: 127.0.0.1:9870
```

#### New

```yaml
sinks:
  hdfs:
    type: webhdfs
    endpoint: http://127.0.0.1:9870
```

authors: thomasqueirozb
