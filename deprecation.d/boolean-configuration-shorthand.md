---
what: "Boolean shorthand for object-valued configuration options"
deprecated_since: "0.60.0"
---

Boolean shorthand for object-valued configuration options is deprecated. Use the object form instead
so that each setting has one consistent representation and the options being configured are explicit.

Existing boolean values continue to work. **We will allow at least two minor release cycles for users
to migrate before removing this shorthand. No removal release has been scheduled.**

This applies to `healthcheck` on sinks, global and sink `acknowledgements`, `jetstream` on the NATS sink,
and `acknowledgements` on the Splunk HEC source.
Ordinary boolean options, such as `healthcheck.enabled`, are unaffected.

Please also see the [dedicated note for `use_otlp_decoding`](/deprecations/#opentelemetry-use_otlp_decoding).
Its planned replacement is handled separately; follow that note instead of migrating through an
intermediate object form.

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

### Already-deprecated source acknowledgements

Other source-level `acknowledgements` settings are already deprecated and have no effect. Remove those
settings and configure acknowledgements globally or on the relevant sinks, following the
[end-to-end acknowledgements documentation](/docs/architecture/end-to-end-acknowledgements/).
