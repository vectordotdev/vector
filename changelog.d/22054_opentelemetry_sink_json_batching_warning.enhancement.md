The `opentelemetry` sink now logs a warning at startup when it is configured
with `encoding.codec = json` and `batch.max_events` greater than 1. That
combination produces invalid OTLP request bodies that receivers reject with
HTTP 400. Use `encoding.codec = otlp` (recommended) or set
`batch.max_events = 1`. The sink documentation now includes a "Batching
considerations" section that spells out both paths.

authors: pront
