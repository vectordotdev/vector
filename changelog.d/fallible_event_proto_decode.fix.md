Decoding Vector's native protobuf format (`decoding.codec = "native"`) and disk-buffer records no longer panics when an event variant is missing or unrecognized, or when a float field is `NaN`. Those payloads are rejected, dropped, and reported through existing decode/buffer error telemetry. A `NaN` float in event data or metadata rejects the entire record rather than rewriting the value.

authors: bruceg
