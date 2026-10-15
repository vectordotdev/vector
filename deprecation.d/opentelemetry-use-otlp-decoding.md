---
what: "OpenTelemetry `use_otlp_decoding` configuration option"
deprecated_since: "0.60.0"
---

### OpenTelemetry: `use_otlp_decoding`

The `opentelemetry` source's `use_otlp_decoding` option is deprecated. Use `decoding` instead.
Signals listed in `signal_types` keep their OTLP format; the rest are converted to Vector's native
format. The source still accepts logs, metrics, and traces, with the same choices as before.

We will give users more than sufficient time to migrate.

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

- Leave out `signal_types` to keep all signals in OTLP format, or set it to `[]` to convert all signals
  to Vector's native format.
- If you currently use `use_otlp_decoding: false`, remove it and leave out `decoding`. Native conversion
  remains the default.
- Use `codec: otlp` and the signal names `logs`, `metrics`, and `traces`. Other values are not supported.
- Remove `use_otlp_decoding` when adding `decoding`. Setting both prevents Vector from starting.
