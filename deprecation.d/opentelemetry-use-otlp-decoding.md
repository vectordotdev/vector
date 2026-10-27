---
what: "OpenTelemetry `use_otlp_decoding` configuration option"
deprecated_since: "0.60.0"
---

<a id="opentelemetry-use_otlp_decoding"></a>

The `opentelemetry` source's `use_otlp_decoding` option is deprecated.
Switch to `decoding` with `codec: otlp` to preserve the same behavior as `use_otlp_decoding: true`.

To preserve the OTLP representation of all signals, replace:

```yaml
use_otlp_decoding: true
```

with:

```yaml
decoding:
  codec: otlp
```

To preserve OTLP only for selected signals, replace:

```yaml
use_otlp_decoding:
  logs: false
  metrics: false
  traces: true
```

with:

```yaml
decoding:
  codec: otlp
  signal_types: ["traces"]
```

Existing boolean and object settings still work, but produce a deprecation warning.

When migrating:

- Leave out `signal_types` to keep all signals in OTLP format. Any
  types not included will remain in Vector's native format.
- If you currently use `use_otlp_decoding: false`, remove it and leave out `decoding`. Native conversion
  remains the default.
- Use `codec: otlp` and the signal names `logs`, `metrics`, and `traces`. Other values are not supported.
- Remove `use_otlp_decoding` when adding `decoding`. Setting both prevents Vector from starting.
