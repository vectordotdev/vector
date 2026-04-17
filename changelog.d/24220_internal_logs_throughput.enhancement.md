Improved the throughput of the `internal_logs` source under high log volume. Broadcast consumption is now decoupled from downstream sending via a dedicated drain task plus a bounded intermediate queue, and events are forwarded downstream in batches. Under a `VECTOR_LOG=trace` firehose this reduces dropped internal log events by roughly 60%.

authors: thomasqueirozb
