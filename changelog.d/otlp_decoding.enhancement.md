The `opentelemetry` source now supports a new decoding mode which can be enabled by setting `use_otlp_decoding` to `true`. In this mode,
all events will preserve the [OTLP](https://opentelemetry.io/docs/specs/otel/protocol/) format. These events can be forwarded directly to
the `opentelemetry` sink without modifications.

A caveat here is that OTLP metrics and Vector metric format differ and thus we treat as logs as they come out the source. These events
cannot be used with existing metrics transforms. However, these can be ingested by the OTEL collectors as metrics.

authors: pront
