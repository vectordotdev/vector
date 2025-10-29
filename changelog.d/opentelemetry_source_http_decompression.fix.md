Fixed a `opentelemetry` source bug where HTTP payloads were not decompressed according to the request headers.
This only applied when `use_otlp_decoding` (recently added) was set to `true`.

authors: pront
