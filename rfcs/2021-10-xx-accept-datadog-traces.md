# RFC <issue#> - 2021-10-xx - Accept Datadog Traces

This RFC describes a change that would:
* Bring initial support for traces
* Ingest traces from the Datadog trace agent (a.k.a. the APM)
* Future support to receive traces from datadog library is accounted for

## Context
This RFC is part of the global effort to enable Vector to ingest & process traffic coming out of Datadog Agents. Vector
internal tracing has its own
[RFC](https://github.com/vectordotdev/vector/blob/97f8eb7/rfcs/2021-08-13-8025-internal-tracing.md).

### Some details about traces handling by the Agent
Traces are collected by a dedicated agent (the `trace-agent`). It has its own runtime, comes with a log of configuration
settings but it shares global option like `site` to select the Datadog region where to send data.

It exposes a [local API](https://github.com/DataDog/datadog-agent/blob/main/pkg/trace/api/endpoints.go), that will be
used by [tracing
libraries](https://docs.datadoghq.com/developers/community/libraries/#apm--continuous-profiler-client-libraries) to
submit traces & profiling data.

It has several communication channels to Datadog:
* Traces are relayed to `trace.<SITE>` (can be overridden by the `apm_config.apm_dd_url` config key)
* Additional profiling data are relayed to `intake.profile.<SITE>` (can be overridden by the
  `apm_config.profiling_dd_url` config key), there not processed by the trace-agent and [relayed directly to
  Datadog](https://github.com/DataDog/datadog-agent/blob/44eec15/pkg/trace/api/endpoints.go#L83-L86)
* Some debug log are simply
  [proxified](https://github.com/DataDog/datadog-agent/blob/44eec15/pkg/trace/api/endpoints.go#L96-L98) to the log
  endpoint `http-intake.logs.<SITE>` (can be overridden by the `apm_config.debugger_dd_url` config key), it is fairly
  [recent](https://github.com/DataDog/datadog-agent/blob/7.31.x/CHANGELOG.rst?plain=1#L60), tracing libs may not use it
  yet.
* It emits [metrics](https://docs.datadoghq.com/tracing/troubleshooting/agent_apm_metrics/) over
  [dogstatsd](https://github.com/DataDog/datadog-agent/tree/44eec15/pkg/trace/metrics)

Profiling and Tracing are enabled independently on traced applications. But they can be correlated once ingested at
Datadog, mainly to refine a span with profiling data.

Additional details on traces, the trace agent encodes data using protobuf, .proto are located in the [datadog-agent
repository](https://github.com/DataDog/datadog-agent/blob/0a19a75/pkg/trace/pb/trace_payload.proto). Trace-agent
requests to the trace endpoint contain two major kind of data:
- standard traces that consist in a an aggregates of spans (cf.
  [[1](https://github.com/DataDog/datadog-agent/blob/0a19a75/pkg/trace/pb/trace_payload.proto#L11)] &
  [[2](https://github.com/DataDog/datadog-agent/blob/0a19a75/pkg/trace/pb/trace.proto#L7-L12)])
- it also sends "selected" spans (a.k.a. APM events) (cf.
  [[3](https://github.com/DataDog/datadog-agent/blob/0a19a75/pkg/trace/pb/trace_payload.proto#L12)], they are extracted
  [here](https://github.com/DataDog/datadog-agent/blob/0a19a75/pkg/trace/event/processor.go#L47-L91))

## Cross cutting concerns
N/A

## Scope

### In scope
- Ingest traces from the trace-agent (`datadog_trace` source)
- Send traces to the Datadog trace endpoint (`datadog_trace` sink)
- Basic operation on traces: VRL, filtering, routing
- Pave the way for OpenTelemetry traces

### Out of scope
- Pofiling data, but as the trace-agent only proxifies profiling data, the same behaviour can be impletement rather
  quickly in Vector.
- Debugger logs can be already diverted to Vector (untested but it should work as vector support datadog logs and there
  is an config option to explicitly configure the debugger log destination)
- Metrics emitted by the trace-agent (they could theoretically be received by Vector by a statsd source, but the host
  use by the trace-agent is derived from the local config to programmatically discover the main agent, thus there is no
  existing knob to force the trace agent to send metric to a custom dogstatsd host)
- Span extraction/filtering
- Other sources & sinks for traces than `datadog_trace`

## Pain

- Vector does not support any traces (full json representation may be ingested as log event) at the moment and it is a
  key part of observability

## Proposal

### User Experience

- User will be able to ingest trace from the trace agent
  - Vector config would then consist in: `datadog_trace` source -> some filtering/enrichment transform ->
    `datadog_trace` sink
  - Datadog trace agent can be configured to send traces to any arbitrary endpoint using the `apm_config.apm_dd_url`
    [config key](https://github.com/DataDog/datadog-agent/blob/34a5589/pkg/config/apm.go#L61-L87)
- This change is a pure addition to Vector, there will be no impact on existing feature

### Implementation

The first item to be address would be to add a new event type that will represent traces, this would materialise as a
new member of the `Event` enum. It shall allow to keep APM events (specific spans extracted by the trace-agent) correlated with their original traces. To allow lossless operation, given some discrepancy between the Datadog trace format and, for example, opentelemetry traces:
```
enum Trace {
    Datadog(DatadogTrace),
    OpenTelemetry(OTTrace),
    [...]
}
```
  This would allow subsequent extension to other kind of trace with specific data structure (perf, ctf, etc.) and
  express clear conversion capabilities.
- Implement a `datadog_trace` source that decodes incoming protobuf to the internal represention implemented in the step
  before, .proto are located in the [datadog-agent
  repository](https://github.com/DataDog/datadog-agent/blob/0a19a75/pkg/trace/pb/trace_payload.proto)
- Implement a `datadog_trace` sink that do the opposite conversion and send the trace to datadog to the relevant region
  according to the sink config


- Explain your change as if you were presenting it to the Vector team.
- When possible, demonstrate with psuedo code not text.
- Be specific. Be opinionated. Avoid ambiguity.

## Rationale

- traces support is expected by users
- local sampling is an interesting feature to lessen the amount of data sent to Datadog

## Drawbacks

- Adding a brand new datatype has a large impact, although it will be mostly code addition

## Prior Art

- Internal rust traces can be converted into [log
  event](https://github.com/vectordotdev/vector/blob/bd3d58c/lib/vector-core/src/event/log_event.rs#L402-L432), but this
  is not reversible, but this is a good way of getting text based representation


## Alternatives

- Implement a vector exporter for the opentelemetry collector and heavily rely on opentelemetry traces structure for
  internal representation
- Regarding implementation trace could also be represented as a new Value type, serialisation to json should not be a
  problem for traces/spans

## Outstanding Questions

- Confirm that this RFC only addresses traces
- Clearly identify contraints over extracted spans (a.k.a APM events):
  - Are they mandatory, i.e. can we just dropped those for the initial implementation
  - Do they need to be sent with their parent trace in the same request
  - What the usual relative amount of extracted spans vs. full traces

## Plan Of Attack

- [ ] Submit a PR introducing the trace event type
- [ ] Submit a PR introducing the `datadog_trace` source & sink

## Future Improvements

- Support for additional trace format
- Profile support
- Ingest traces from app directly
- Opentelemetry exporter support (the Datadog export would probably be easily supported once this RFC has been
  implemented as it's using the same [Datadog endpoint thant the trace
  agent](https://github.com/open-telemetry/opentelemetry-collector-contrib/blob/04f97ec/exporter/datadogexporter/config/config.go#L288-L290)
- Traces helper in VRL
