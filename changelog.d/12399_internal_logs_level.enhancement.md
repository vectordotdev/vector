Add a `level` option to the `internal_logs` source that controls the maximum verbosity of log
events delivered to the source, decoupling it from the console log level. Previously, the source
only received the log events selected by `VECTOR_LOG`, `--verbose`, and `--quiet`; it now receives
log events at the configured `level` (defaulting to `info`) regardless of those startup options,
so, for example, logs can still be collected by the source when console logging is disabled with
`VECTOR_LOG=off`.

authors: dekelpilli
