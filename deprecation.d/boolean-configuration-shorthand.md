---
what: "Boolean shorthand for object-valued configuration options"
deprecated_since: "0.60.0"
---

Boolean shorthand for object-valued configuration options is deprecated. Use the object form instead
so that each setting has one consistent representation and the options being configured are explicit.

Existing boolean values continue to work. **We will allow at least two minor release cycles for users
to migrate before removing this shorthand. No removal release has been scheduled.**

This applies to `healthcheck` on sinks, global and sink `acknowledgements`, `jetstream` on the NATS sink,
`acknowledgements` on the Splunk HEC source, and `use_otlp_decoding` on the OpenTelemetry source.
Ordinary boolean options, such as `healthcheck.enabled`, are unaffected.

### Settings with an `enabled` field

For `healthcheck`, global and sink `acknowledgements`, NATS sink `jetstream`, and Splunk HEC source
`acknowledgements`, replace the boolean with an object containing the same value in `enabled`.
For example, within a sink configuration, replace:

```yaml
healthcheck: false
acknowledgements: true
```

with:

```yaml
healthcheck:
  enabled: false
acknowledgements:
  enabled: true
```

Other options in these objects retain their defaults. Use the same transformation for either `true`
or `false`.

### OpenTelemetry: `use_otlp_decoding`

This option controls which signals preserve their OTLP representation instead of being converted to
Vector's native representation. Its boolean shorthand sets **all three signal options**.

#### Planned array form

The planned interface is an array of signal names. **Array support is not implemented yet; do not use
these array values until support is released.** The intended migration is:

| Existing configuration | Planned array form |
| --- | --- |
| `use_otlp_decoding: false` | `use_otlp_decoding: []` |
| `use_otlp_decoding: true` | `use_otlp_decoding: ["logs", "metrics", "traces"]` |
| `use_otlp_decoding: {logs: true, metrics: false, traces: true}` | `use_otlp_decoding: ["logs", "traces"]` |
| Option omitted | Leave it omitted; all signals continue to use native conversion |

An explicit array enables OTLP decoding only for the listed signals. An empty array and an omitted
option both mean no signals use OTLP decoding, preserving the current default. Array order has no
meaning, and unknown signal names will be rejected.

This setting is separate from the OTLP codec's `decoding.signal_types` array, which controls the
signal formats the codec tries to parse and their priority.

#### Migration available today

Until array support is released, use the supported per-signal object form. Replace:

```yaml
use_otlp_decoding: true
```

with:

```yaml
use_otlp_decoding:
  logs: true
  metrics: true
  traces: true
```

When migrating `use_otlp_decoding: false`, set all three options to `false` instead.

### Already-deprecated source acknowledgements

Other source-level `acknowledgements` settings are already deprecated and have no effect. Remove those
settings and configure acknowledgements globally or on the relevant sinks, following the
[end-to-end acknowledgements documentation](/docs/architecture/end-to-end-acknowledgements/).
