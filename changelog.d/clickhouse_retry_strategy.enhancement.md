The `clickhouse` sink now supports the `retry_strategy` option, matching the `http` sink. This controls which HTTP responses are treated as retriable.

This matters for sources with end-to-end acknowledgements. A non-retriable response makes the sink drop the batch, finalizing it as `Rejected`. For the `kafka` source that is not inert: offsets are a single watermark per partition, so the next batch that succeeds stores an offset past every rejected batch behind it, and those events are lost with consumer lag never reflecting it. Retrying instead means the batch never resolves, so the watermark cannot advance past it.

The default strategy treats any non-5xx other than 429/408 as non-retriable, including the 404 that ClickHouse returns for an unknown table. Sinks that must not lose events can now opt into retrying those:

```yaml
retry_strategy:
  type: custom
  status_codes: [401, 403, 404, 408, 429]
```

ClickHouse reports malformed data as a 500 with a `Code: 117` or `Code: 53` body. Those remain non-retriable under every strategy, since retrying a poison pill would block every batch queued behind it.

authors: jamesdangercarpenter
