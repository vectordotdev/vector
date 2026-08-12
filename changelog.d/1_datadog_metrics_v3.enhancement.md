Adds a new encoder to the Datadog metrics sink to encode metrics with v3 of
the payload protocol. An additional option `dual_write` will make Vector send
duplicate series payloads to the given endpoint encoded with the configured
protocol. This allows the Datadog backend to validate that the metrics sent via
both protocols specify the exact same metrics. Only series metrics are
dual-written; sketches are never shadowed.

authors: stephenwakely
