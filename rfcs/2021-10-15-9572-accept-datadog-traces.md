# RFC 9572 - 2021-10-15 - Accept Datadog Traces

This RFC describes a change that would:

* Bring initial support for traces
* Ingest traces from the Datadog trace agent (a.k.a. the client side of the APM product)
* Relay ingested traces to Datadog

## Context

This RFC is part of the global effort to enable Vector to ingest & process traffic coming out of Datadog Agents. Vector
internal tracing has its own
[RFC](https://github.com/vectordotdev/vector/blob/97f8eb7/rfcs/2021-08-13-8025-internal-tracing.md).

### Some details about traces handling by the Agent

Traces are collected by a dedicated agent (the `trace-agent`). It has its own runtime, comes with a log of configuration
settings but it shares global option like `site` to select the Datadog region where to send data.

It exposes a [local API](https://github.com/DataDog/datadog-agent/blob/main/pkg/trace/api/endpoints.go), that is used by
[tracing
libraries](https://docs.datadoghq.com/developers/community/libraries/#apm--continuous-profiler-client-libraries) to
submit traces & profiling data.

It has several communication channels to Datadog:

* Processed traces are relayed to `trace.<SITE>` (can be overridden by the `apm_config.apm_dd_url` config key)
* Additional profiling data are relayed to `intake.profile.<SITE>` (can be overridden by the
  `apm_config.profiling_dd_url` config key), they are not processed by the trace-agent and [relayed directly to
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

The trace-agent encodes data using protobuf, .proto are located in the [datadog-agent
repository](https://github.com/DataDog/datadog-agent/blob/0a19a75/pkg/trace/pb/trace_payload.proto). Trace-agent
requests to the trace endpoint contain two major kind of data:

* standard traces that consist of an aggregate of spans (cf.
  [[1](https://github.com/DataDog/datadog-agent/blob/0a19a75/pkg/trace/pb/trace_payload.proto#L11)] &
  [[2](https://github.com/DataDog/datadog-agent/blob/0a19a75/pkg/trace/pb/trace.proto#L7-L12)])
* it also sends "selected" spans (a.k.a. APM events) (cf.
  [[3](https://github.com/DataDog/datadog-agent/blob/0a19a75/pkg/trace/pb/trace_payload.proto#L12)], they are extracted
  [here](https://github.com/DataDog/datadog-agent/blob/0a19a75/pkg/trace/event/processor.go#L47-L91)), once ingested by
  Datadog those selected spans are used under the hood to better identify & contextualize traces, they can be fully
  indexed as well ([short description of APM
  events](https://github.com/DataDog/datadog-agent/blob/e081bed/pkg/trace/event/doc.go)))

## Cross cutting concerns

N/A

## Scope

### In scope

* Ingest traces from the trace-agent (`datadog_trace` source)
* Send traces to the Datadog trace endpoint (`datadog_trace` sink)
* Basic operation on traces: filtering, routing
* Pave the way for OpenTelemetry traces

### Out of scope

* Pofiling data, but as the trace-agent only proxifies profiling data, the same behaviour can be impletement rather
  quickly in Vector.
* Debugger logs can be already diverted to Vector (untested but it should work as vector support datadog logs and there
  is an config option to explicitly configure the debugger log destination)
* Metrics emitted by the trace-agent (they could theoretically be received by Vector by a statsd source, but the host
  use by the trace-agent is derived from the local config to programmatically discover the main agent, thus there is no
  existing knob to force the trace agent to send metric to a custom dogstatsd host)
* Span extraction, filtering
* Other sources & sinks for traces than `datadog_trace`

## Pain

* Vector does not support any traces (full json representation may be ingested as log event) at the moment and it is a
  key part of observability

## Proposal

### User Experience

* User will be able to ingest trace from the trace agent
  * Vector config would then consist in: `datadog_trace` source -> some filtering/enrichment transform ->
    `datadog_trace` sink
  * Datadog trace agent can be configured to send traces to any arbitrary endpoint using the `apm_config.apm_dd_url`
    [config key](https://github.com/DataDog/datadog-agent/blob/34a5589/pkg/config/apm.go#L61-L87)
* This change is a pure addition to Vector, there will be no impact on existing feature

### Implementation

The first item to be address would be to add a new event type that will represent traces, this would materialise as a
new member of the `Event` enum. As it would be implemented in vector-core, it's probably better to stay relatively
vendor agnostic, so basing it on the [OpenTelemetry trace
format](https://github.com/open-telemetry/opentelemetry-proto/blob/main/opentelemetry/proto/trace/v1/trace.proto) with
additional field upon necessity is probably a safe option. Overal there is no huge discrepancy between Datdog traces and
OpenTelemetry traces (The trace-agent already offer
[OLTP->Datadog](https://github.com/DataDog/datadog-agent/blob/637b43e/pkg/trace/api/otlp.go#L305-L377) conversion). The main difference is that Datadog spans come with a string/double map containing metrics and string/string map for some metadata whereas OTLP traces comes with a list of key/value (value mimic json values). The easiest way do deal with that would be for the Vector trace struct to keep the OLTP generic key/value list as generic structured metadata holder along with a tags maps for Datadog format string/string map and a the string/double metric map.

This `Trace` struct shall allow to represent APM events (specific spans extracted by the trace-agent), so this `Trace`
struct has to support standalone spans/or single span traces.

Based on the aforementioned work a source & sink would then be added to Vector:

* A `datadog_trace` source that decodes incoming [gzip'ed protobuf over
  http](https://github.com/DataDog/datadog-agent/blob/8b63d85/pkg/trace/writer/trace.go#L230-L269) to the internal
  represention implemented in the step before. .proto are located in the [datadog-agent
  repository](https://github.com/DataDog/datadog-agent/blob/0a19a75/pkg/trace/pb/trace_payload.proto)
* A `datadog_trace` sink that does the opposite conversion and sends the trace to datadog to the relevant region
  according to the sink config

Datadog API key management would be the same as it is for Datadog logs & metrics.

## Rationale

* traces support is expected by users
* local sampling is an interesting feature to lessen the amount of data sent to Datadog

## Drawbacks

* Adding a brand new datatype has a large impact, although it will be mostly code addition, it will impact vector-core

## Prior Art

* Internal rust traces can be converted into [log
  event](https://github.com/vectordotdev/vector/blob/bd3d58c/lib/vector-core/src/event/log_event.rs#L402-L432), but this
  is not reversible, but this is a good way of getting text based representation

## Alternatives

* Regarding implementation traces could also be represented as a log event, conversion to/from json should
  theroretically not be a problem for traces/spans, but it would generate deeper than usual structure (should not be a
  problem though)
* Traces could be represented themselves as a enum with specific implementation per vendor, allowing almost direct
  mapping from protocol definition into rust struct.

## Outstanding Questions

* Confirm that this RFC only addresses traces
* Do we want to reuse part of [this OpenTelemetry project](https://github.com/open-telemetry/opentelemetry-rust)

## Plan Of Attack

* [ ] Submit a PR introducing the trace event type
* [ ] Submit a PR introducing the `datadog_trace` source & sink

## Future Improvements

* Support for additional trace formats, probably OpenTelemetry first
* Profile support
* Ingest traces from app directly
* Opentelemetry exporter support (the Datadog export would probably be easily supported once this RFC has been
  implemented as it's using the same [Datadog endpoint thant the trace
  agent](https://github.com/open-telemetry/opentelemetry-collector-contrib/blob/04f97ec/exporter/datadogexporter/config/config.go#L288-L290)
* Traces helpers in VRL
* Trace-agent configuration with a `vector.traces.url` & `vector.traces.enabled`
