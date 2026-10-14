---
what: "OpenTelemetry `use_otlp_decoding` configuration option"
deprecated_since: "0.60.0"
---

### OpenTelemetry: `use_otlp_decoding`

The `opentelemetry` source's `use_otlp_decoding` option is deprecated in favor of `decoding`.
Existing boolean and per-signal object values continue to work and produce a deprecation warning.
Removal is not yet scheduled. We will give users at least one month to migrate before removing this
option.

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

All three signals remain accepted. Listed signals retain their OTLP representation; unlisted signals
are converted to Vector's native representation. This preserves every existing per-signal combination.
OTLP metrics remain log events and cannot be processed by native metric transforms.

Omitting `signal_types` selects all three signals. An empty list uses native conversion for all
signals. List order has no effect, and duplicate names are ignored.

To migrate `use_otlp_decoding: false`, remove the option and omit `decoding`. The default remains
native conversion for all signals.

Only `codec: otlp` is supported by this source. Other codecs and unknown signal names are rejected.
Remove `use_otlp_decoding` when adding `decoding`; configuring both is an error at startup.
