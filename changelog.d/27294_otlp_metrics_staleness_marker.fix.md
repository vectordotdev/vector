The `otlp` encoding codec no longer rejects a `NaN` metric value as malformed. `NaN` is a valid
signal under the Prometheus remote-write 1.0 spec: senders mark a series as stale with a specific
`NaN` bit pattern when it will no longer receive new samples, and `prometheus_remote_write` sources
pass that bit pattern straight through as a Vector metric value. Previously, any sink using
`encoding.codec: otlp` (for example `http` or `opentelemetry`) raised an `encoder_serialize` error
and dropped the event for every staleness marker it saw, which autoscaling, pod restarts, and
cardinality changes produce continuously.

A `NaN` counter or gauge value now encodes as a zero-valued OTLP point flagged with
`NO_RECORDED_VALUE`, OTLP's own equivalent of the Prometheus staleness marker. The `opentelemetry`
source's decode path is now the mirror of this: a data point flagged `NO_RECORDED_VALUE` decodes
to the Prometheus staleness `NaN` instead of a fabricated `0.0`, so the source can newly emit `NaN`
metric values where it previously emitted zero.

A `NaN` histogram sum is omitted (OTLP's histogram `sum` is optional) and only flags
`NO_RECORDED_VALUE` when the histogram has no observations; a histogram with real bucket counts
keeps those counts and is not flagged. A `NaN` anywhere in a summary (its sum or any quantile
value) flags the whole point and stales every quantile value, since OTLP has no way to mark a
single quantile as stale.

This does not fix the case of a `prometheus_remote_write` source receiving a stale histogram or
summary: Vector's remote-write parser already rejects a `NaN` `_count` for those metric types with
an HTTP 400 before this codec ever sees the data. That is a separate, pre-existing issue.

authors: julienvey
