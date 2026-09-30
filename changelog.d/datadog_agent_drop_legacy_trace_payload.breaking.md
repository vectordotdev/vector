# `datadog_agent` source no longer accepts pre-`tracerPayloads` trace payloads {#datadog-agent-legacy-trace-payload}

## Summary

The `datadog_agent` source no longer accepts the pre-`tracerPayloads` Agent-to-intake trace protobuf
(`traces` / `transactions` fields). The Datadog Agent dropped those fields in the 7.33.0 release in
January 2022. An empty `tracerPayloads` list now produces no events and increments
`component_errors_total` with `error_code` `empty_tracer_payloads`. Indexed `idxTracerPayloads`
entries are recognized but not converted (`error_code` `idx_tracer_payloads`).

## Migration

Upgrade the Datadog Agent to 7.33.0 or later. If you already run a current Agent, no action is
needed. Indexed `idxTracerPayloads` payloads are recognized but not converted; those traces are
dropped with `error_code` `idx_tracer_payloads`.

authors: bruceg
