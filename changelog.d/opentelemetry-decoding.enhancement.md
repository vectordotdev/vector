Add `decoding.codec: otlp` and `decoding.signal_types` to the `opentelemetry` source, preserving OTLP
for selected signals while keeping native conversion for the others. The default remains native
conversion for all signals. Existing `use_otlp_decoding` configurations remain supported but are
deprecated; see the dedicated migration notice.

authors: pront
