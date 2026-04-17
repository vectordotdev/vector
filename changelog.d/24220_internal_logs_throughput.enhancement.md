Improved the throughput of the `internal_logs` source under high log volume. When using `VECTOR_LOG=trace`, which produces a very high amount of logs, this reduces dropped internal log events by roughly 60%.

authors: thomasqueirozb
