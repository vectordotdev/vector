# RFC xyz - 2022-03-xx - Opentelemetry traces source

This RFC aims to describes how to add an OpenTelemetry traces source to Vector and also address Vector internals
adjustement required for future extension to other trace types.

## Context

- `datadog_agent` source supports receiving traces from the Datadog `trace-agent`
- `datadog_traces` sink supports emitting traces to Datadog
- OpenTelemetry traces are already supported by Datadog:
  - Either with the Datadog exporter using the opentelemetry collector (without the `trace-agent`)
  - Or with the `trace-agent` configured to receive OpenTelemtry traces (both grpc and http transport layer are
    supported)

## Cross cutting concerns

- Link to any ongoing or future work relevant to this change.

## Scope

### In scope

- `opentelemetry_traces` source, with both http and grpc support
- `opentelemetry_traces` source to `datadog_traces` sink forwarding
- Settle on a signle internal representation for all traces form inside vector
- APM stats computation logic, with an implementation for the `opentelemetry_traces` sources, applicable for all traces
  sources


### Out of scope

N/A

## Pain

- Avoid complex setup when ingesting traces, ultimately pointing every tracing lib directly to Vector should just work
  out-of-the-box with minimal config.

## Proposal

### User Experience

- User would point OpenTelemtry tracing lib directly to a local Vector deployement
- Vector would be configured with a config looking like:

```yaml
sources:
  otlp_traces:
    type: opentelemetry_traces
    address: "[::]:8081"

sinks:
  dd_trace:
    type: datadog_traces
    default_api_key: 12345678abcdef
    inputs:
     - otlp_traces
```

And it should just work.

### Implementation

- `opentelemetry_source`:
  - grpc details TBD
  - http details TBD
- Internal traces representation/normalization: TBD
- APM stats computation: likely to mimic what's done in the Datadog OTLP exporter

## Rationale

- Opentelemtry is the de-facto standard for traces, so supporting it at some point is mandatory.

## Drawbacks

N/A

## Prior Art

N/A

## Alternatives

- We could keep the Datadog trace-agent as an OTLP->Datadog traces converter and ingest datadog traces from there
- We could keep the Datadog exporter as an OTLP->Datadog traces converter and ingest datadog traces from there
- We could write a Vector exporter for the Opentelemetry collector, note that this would likely leverage the Vector protocol and this logic could be applied to metrics as well

## Outstanding Questions

- How do we do traces normalization/format enforcement
- Do we want to have a single `opentelemtry` source with names output or multiple sources ?
- APM stats computation:
  - Either in source (to be done for each source, except for the `datadog_agent` sources where APM stats may be decoded from received payloads) - likely to be the preferred solution
  - Either in a transform like `traces_to_metrics`

## Plan Of Attack

- [ ] Implement traces normalisation/schema
- [ ] `opentelemetry_traces`, http mode
- [ ] `opentelemetry_traces`, grpc mode
- [ ] APM stats computation : either in `opentelemtry_traces` or in a dedicated transform

## Future Improvements

- Transforms / VRL helpers to manipulate traces or isolate outliers
- OpenTelemtry sinks