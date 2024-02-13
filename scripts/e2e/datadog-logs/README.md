This e2e test covers the `datadog_agent` source, and the
`datadog_logs` sink.

Fake logs are generated in the emitter service and written
to a file.

Two Agent containers are spun up to read the log file, one
for the Agent only case and one for the Agent -> Vector case.

In the Agent only case, the Agent sends the logs to `fakeintake`
(another service) directly. This is the baseline.

In the Agent-Vector case, the Agent send the logs to the vector
service, and the `datadog_logs` sink sends to a separate
`fakeintake` service. This is the compare case.

The two sets of data should be shaped the same in terms of when
the events were received, and the content of the events, but the
timestamps themselves are not guaranteed to align.
