Preserve native OTLP log event names in the `opentelemetry` source. Decoded logs expose
`event_name`, while OTLP decoding preserves `eventName`; no duplicate log attribute is needed.

authors: bpceee
