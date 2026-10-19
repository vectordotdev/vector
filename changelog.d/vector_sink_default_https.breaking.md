# Scheme-less `vector` sink endpoints default to `https` {#vector-sink-default-https}

## Summary

`address` and `routing.endpoints` values in the `vector` sink that omit the scheme (for example,
`127.0.0.1:6000` instead of `http://127.0.0.1:6000`) now default to `https`. Previously they
defaulted to `http` unless TLS was enabled. The `http` default was
[deprecated in 0.59.0](https://vector.dev/deprecations/). Configurations that already enable TLS
are unaffected. A scheme-less endpoint pointing at a plaintext `vector` source now fails the TLS
handshake.

## Migration

Specify `http://` explicitly to keep connecting without TLS. Setting `tls.enabled: false` alone
does not restore the old default.

#### Old

```yaml
sinks:
  my_sink:
    type: vector
    inputs: [in]
    routing:
      endpoints: ["127.0.0.1:6000", "127.0.0.1:6001"]
```

#### New

```yaml
sinks:
  my_sink:
    type: vector
    inputs: [in]
    routing:
      endpoints: ["http://127.0.0.1:6000", "http://127.0.0.1:6001"]
```

The same applies to `address`, for example, `address: http://127.0.0.1:6000`.

authors: JinchengCao
