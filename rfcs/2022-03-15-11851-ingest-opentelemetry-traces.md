# RFC 11851 - 2022-03-15 - Opentelemetry traces source

This RFC aims to describes how to add an OpenTelemetry traces source to Vector and also address Vector internals
adjustment required for future extension to other trace types.

## Context

- `datadog_agent` source supports receiving traces from the Datadog `trace-agent`
- `datadog_traces` sink supports emitting traces to Datadog
- OpenTelemetry traces are already supported by Datadog:
  - Either with the [Datadog exporter][otlp-dd-exporter] using the Opentelemetry collector (without the `trace-agent`)
  - Or with the `trace-agent` [configured to receive OpenTelemetry traces][otlp-traces-with-dd-agent] (both gRPC and HTTP
    transport layer are supported)

### Usecases

As the whole traces processing inside Vector is pretty new, documenting confirmed and most credible use cases in the
near future will help to ensure changes will be implemented so they will be really useful to potential users. This also
help to build something flexible enough to accommodate future needs.

One identified scenario is to demux a trace flow based on some conditions that could be evaluated against any metadata
for a single trace, a group of traces or per spans. From a config perspective this would expect to be functional with
the following configuration:

```yaml
[...]
sources:
  otlp:
    type: opentelemetry
    address: "[::]:8081"
    mode: grpc

transforms:
  set_key:
    type: remap
      source: |
        if exists!(.tags.user_id) {
          return
        }
        key = get_enrichment_table_record!("api_keys", { "user": .tags.user_id })
        set_metadata_field("datadog_api_key", key)
      inputs:
       - otlp.traces # Would exclusively emit traces

sinks:
  dd_trace:
    type: datadog_traces
    default_api_key: 12345678abcdef
    inputs:
      - set_key
```

This demux/conditional action can be seen as an extension of what currently exists in Vector. Other kind of conditional
action like the `filter` transform to discard traces base on certain metadata can be considered to be very similar, as
this also involve evaluating a VRL condition on traces. The key problem here is to exposes traces and spans field in a
way that the user can still manipulate those easily.

This however raises the case of the granularity of a single event ; for instance multiple traces can bundle into a
single payload in both OpenTelemetry and Datadog wire format. Enabling clear processing without ambiguity advocate for a
clear constraint that should be enforced by all future traces sources : **a single Vector event shall not hold data
relative to more that one trace**.

A completely different usecase is traces sampling, but it cover two major variations:

- Simple sampling: either cap/pace the trace flow at a given rate or sample 1 trace per 10/100/1000/etc. traces, and
  this is already available thanks to the `sample` and `throttle` transforms
- Outliers isolation, this would mean keeping some traces based on some advanced criteria, like execution time above
  p99, this would require comparison against histogram / sketches.

Another valuable identified usecase is the ability to provide seamless conversion between any kind of Vector supported
traces, this means that the Vector internal traces representation shall be flexible enough to accommodate conversion
to/from any trace format in sources and sinks that work with traces. Given the traction from the Opentelemetry project,
and the fact that it [comes with a variety of fields][otlp-trace-proto-def] to cover most usecases.

## Cross cutting concerns

N/A

## Scope

### In scope

- `opentelemetry` source, with both http and grpc support, decoding traces only, but with provision for other datatypes and emitting traces on a named output `traces`
- Support `opentelemetry` source to `datadog_traces` sink forwarding by dealing with:
  - Traces normalization to a single format inside Vector
  - Conversion to/from this format in all traces sources/sinks
- APM stats computation logic, with an implementation fully functional for traces from both the `opentelemetry` and the
  `datadog_agent` sources.

### Out of scope

N/A

## Pain

- Avoid complex setup when ingesting traces, ultimately pointing every tracing lib directly to Vector should just work
  out-of-the-box with minimal config.

## Proposal

### User Experience

- User would point OpenTelemetry tracing lib directly to a local Vector deployment
- Vector would be configured with a minimal config looking like:

```yaml
sources:
  otlp:
    type: opentelemetry
    address: "[::]:8081"
    mode: grpc

sinks:
  dd_trace:
    type: datadog_traces
    default_api_key: 12345678abcdef
    inputs:
     - otlp.traces # Would exclusively emit traces
```

And it should just work.

### Implementation

Based on the [usecases](#usecases) previously detailed the implementation will we can extract the following
top-level requirements:

- A Vector trace event shall only contain data relative to one single trace, i.e. traces sources shall create one event
  for each individual trace ID and its associated spans and metadata.
- Use the Opentelemetry trace format as the common denominator and base the Vector internal representation to ensure :
  - A clear reference point for conversion between trace formats
  - Avoid destructive manipulation by transforms and keep traces object fully functional even after heavy modifications
    while flowing through the topology

#### Source structure

A new `opentelemetry` source with a named output `traces` (future extension would cover `metrics` then `logs`):

- The gRPC variant would use Tonic to spawn a gRPC server (like the `vector` source in its v2 variation) and directly
  use the [official gRPC service definitions][otlp-grpc-def], only the traces gRPC service will be accepted, this should be
  relatively easy to extend it to support metrics and logs gRPC services.
- HTTP variant would use a Warp server and attempt to decode protobuf payloads, as per the [specification][otlp-http],
  payloads are encoded using protobuf either in binary format or in JSON format ([Protobuf schemas][otlp-proto-def]).
  All the expected behaviours regarding the kind of requests/responses code and sequence are clearly defined as well
  as the default URL path (`/v1/traces` for traces, demuxing `/v1/metrics` and `/v1/logs` later should not be a problem).

#### Traces normalization/format enforcement

For cross format operation like `opentelemetry` source `traces` output to `datadog_traces` sinks or the opposite
(Datadog to OpenTelemetry) trace standardization is require so between sinks/sources traces will follow one single
universal representation, there is two major possible approaches:

  1. Stick to a `LogEvent` based representation and leverage [Vector event schema][schema-work]
  2. Move traces away from their current representation (as LogEvent) and build a new container based on a set of
     dedicated structs representing traces and spans with common properties and generic key/value store(s) to allow a
     certain degree of flexibility.

The second option would have to provide a way to store, at least, all fields from both Opentelemetry and Datadog Traces.
If we consider the protobuf definition for both Datadog and OpenTelemetry, it is clear that the OpenTelemetry from come
with extra structured fields that are not present in Datadog traces. However having a generic key/value container in
virtually all traces formats can be used to store data that do not have a dedicated field in some format. As a reflexion
basis the Datadog and OpenTelemetry are provided below, there is no hard semantic differences.

Datadog [newer trace format][otlp-trace-proto-def] (condensed):

```protobuf
message Span {
    string service = 1;
    string name = 2;
    string resource = 3;
    uint64 traceID = 4;
    uint64 spanID = 5;
    uint64 parentID = 6;
    int64 start = 7;
    int64 duration = 8;
    int32 error = 9;
    map<string, string> meta = 10;
    map<string, double> metrics = 11;
    string type = 12;
    map<string, bytes> meta_struct = 13;
}

message TraceChunk {
  // priority specifies sampling priority of the trace.
  int32 priority = 1;
  // origin specifies origin product ("lambda", "rum", etc.) of the trace.
  string origin = 2;
  // spans specifies list of containing spans.
  repeated Span spans = 3;
  // tags specifies tags common in all `spans`.
  map<string, string> tags = 4;
  // droppedTrace specifies whether the trace was dropped by samplers or not.
  bool droppedTrace = 5;
}

// TracerPayload represents a payload the trace agent receives from tracers.
message TracerPayload {
  // containerID specifies the ID of the container where the tracer is running on.
  string containerID;
  // languageName specifies language of the tracer.
  string languageName;
  // languageVersion specifies language version of the tracer.
  string languageVersion = 3 ;
  // tracerVersion specifies version of the tracer.
  string tracerVersion = 4;
  // runtimeID specifies V4 UUID representation of a tracer session.
  string runtimeID = 5;
  // chunks specifies list of containing trace chunks.
  repeated TraceChunk chunks = 6;
  // tags specifies tags common in all `chunks`.
  map<string, string> tags = 7;
  // env specifies `env` tag that set with the tracer.
  string env = 8;
  // hostname specifies hostname of where the tracer is running.
  string hostname = 9;
  // version specifies `version` tag that set with the tracer.
  string appVersion = 10;
}

```

Opentelemetry [trace format][otlp-proto-def] (condensed):

```protobuf
message InstrumentationLibrarySpans {
  opentelemetry.proto.common.v1.InstrumentationLibrary instrumentation_library = 1;
  repeated Span spans = 2;
  string schema_url = 3;
}

message Span {
  bytes trace_id = 1;
  bytes span_id = 2;
  string     = 3;
  bytes parent_span_id = 4;
  string name = 5;

  enum SpanKind {
    SPAN_KIND_UNSPECIFIED = 0;
    SPAN_KIND_INTERNAL = 1;
    SPAN_KIND_SERVER = 2;
    SPAN_KIND_CLIENT = 3;
    SPAN_KIND_PRODUCER = 4;
    SPAN_KIND_CONSUMER = 5;
  }

  SpanKind kind = 6;
  fixed64 start_time_unix_nano = 7;
  fixed64 end_time_unix_nano = 8;
  repeated opentelemetry.proto.common.v1.KeyValue attributes = 9;
  uint32 dropped_attributes_count = 10;

  message Event {
    fixed64 time_unix_nano = 1;
    string name = 2;
    repeated opentelemetry.proto.common.v1.KeyValue attributes = 3;
    uint32 dropped_attributes_count = 4;
  }

  repeated Event events = 11;
  uint32 dropped_events_count = 12;

  message Link {
    bytes trace_id = 1;
    bytes span_id = 2;
    string trace_state = 3;
    repeated opentelemetry.proto.common.v1.KeyValue attributes = 4;
    uint32 dropped_attributes_count = 5;
  }

  repeated Link links = 13;
  uint32 dropped_links_count = 14;
  Status status = 15;
}
```

The key construct in all trace formats is the **span** and traces are a set of spans. The OpenTelemetry span structure
is rather verbose and comes with complex nested field. The [Datadog approach][span-conversion] is either to ignore those
(e.g. the links field is ignored) or encode the complete field into a text representation (e.g. events are encoded using
JSON) and include the resulting value into the tags (a.k.a Meta) map.

This makes the opposite conversion a bit complicated if we want it to be completely symmetrical but there was already an
[attempt][otlp-dd-trace-receiver] allow Datadog traces ingestion in the OpenTelemetry collector. While this PR was
closed unmerged this provide a valuable example. Anyways the [otlp-and-other-formats][OpenTelemetry] acknowledges that
some of the OpenTelemetry construct ends up being stored as tags or annotations in other formats.

Anyway the OpenTelemetry to Datadog traces conversion is dictated by existing implementations in both the `trace-agent`
and the Datadog exporter as users will expect a consistent behaviour from one solution to another. The same
consideration applies for APM stats computation, as [official implementations][apm-stats-computation] already provides a
reference that defines what should be done to get the same result with Vector in the loop. The other way, from Datadog to
OpenTelemetry is less common as of today but while implementing conversions we shall ensure that the following path is
idempotent:

`(Datadog Trace) -> (Vector internal format - based on Opentelemetry) -> (Datadog Trace)`

There is no particular field or subset of metadata that would prevent idempotency in that case. This remains a strong
requirement and shall be applicable to all third party trace format that will be converted to/from the upcoming Vector
internal representation for similar scenarios.

**Note**: The [Rust OpenTelemetry implementation][otlp-rust] implement a conversion from OpenTelemetry traces to the
Datadog `trace-agent` format. This is not the purpose of this RFC, and with the OpenTelemetry traces format being
supported on both sides working on better interoperability on that particular common ground would likely be a better
option.

**Conclusion**: the implementation will stay around [./lib/vector-core/src/event/trace.rs][current-trace-in-vector], it
will borrow most of the OpenTelemetry to allow straightforward trace conversion to the newer Vector internal
representation. Regarding `datadog_agent` source and `datadog_traces` sink the conversion to/from this newer trace
representation will follow existing logic and ensure that standard usecases (like introducing Vector between the Datadog
intake and the `trace agent`) do not significantly change the end-to-end behaviour. Some top-level information (Like
trace ID, trace-wide tags/metrics, the original format) are likely to be added to the internal trace representation for
efficiency and convenience.
Trace would not get native `VrlTarget` representation anymore, there is a bigger discussion there that should probably
be addressed separately. As an interim measure few fields may be exposed (At least trace ID & trace-wide tags), the spans
list will not be exposed initially.

#### APM stats computation

The APM stats computation can be seen as a generic way to compute some statistics on a traces flow, the following key
points have been discussed:

- While [APM stats][apm-stats-proto] may be useful outside Datadog context, as they are somehow standard metrics, and
  they could theoretically be useful to any metric backends, but as of today it seems unlikely that this will ever
  happen. So third-party usage of APM stats won't be addressed until there is demand for it.
- APM stats are essentially a Datadog things, and if metrics should be extracted from traces at some point in the
  feature this would probably materialize as a `traces_to_metric` transform, but the exact scope and the usefulness of
  such a transform remains to be determined. But this will be unrelated to APM stats.
- Considering the user experience, it appears that not exposing any APM stats consideration in Vector config is a safe
  conservative choice. That being said APM stats coming out of Vector shall remain relevant in all circumstances. Based
  on the fact that first identified usecases revolve around routing and filtering the most convenient location to do APM
  Stats computation is directly in the `datadog_traces` sink. The major issue is around sampling, statistically speaking
  distribution metrics wont be impacted, but other metrics (like counter/gauge) will, note that if the sampling rate is
  known it would still be possible to get an original value estimate for those metrics. Anyway this has to be
  documented.
- Implement a similar logic that the one done in the Datadog OTLP exporter, this would allow user to use multiple
  Datadog products with Opentelemetry traces and get the same consistent behaviour in all circumstances. APM stats
  computation is hooked [there][apm-stats-computation] in the Datadog exporter. But as this is go code it relies on the
  [Agent codebase][agent-code-for-otlp-exporter] to do the [actual computation][agent-handle-span].

**Conclusion**: APM stats computation will follow what's done in the [Datadog OTLP exporter][apm-stats-computation] and
the computation will happen against the outgoing traces stream directly in the `datadog_traces` sink. Incoming APM stats
received in the `datadog_agent` will then be ignored.

## Rationale

- Opentelemetry is the de-facto standard for traces, so supporting it at some point is mandatory. Note that this
  consideration is wider than just traces as metrics (and logs) are addressed by the Opentelemetry project.

## Drawbacks

Adopting an internal trace representation based on OpenTelemetry seems well suited for application that involves remote
submission and processing. However for other traces formats a bit far from the OpenTelemetry format, like the
[CTF][common-trace-format], that can also be emitted while the traced application is running, may not fit very well into
an OpenTelemetry-based representation.

## Prior Art

N/A

## Alternatives

- We could keep the Datadog trace-agent as an OTLP->Datadog traces converter and ingest datadog traces from there
- We could keep the Datadog exporter as an OTLP->Datadog traces converter and ingest datadog traces from there
- We could write a Vector exporter for the Opentelemetry collector, note that this would likely leverage the Vector
  protocol and this logic could be applied to metrics as well

## Outstanding Questions

N/A

## Plan Of Attack

- [ ] Implement traces normalisation/schema
- [ ] `opentelemetry` source, gRPC mode
- [ ] `opentelemetry` source, HTTP mode
- [ ] APM stats computation

## Future Improvements

- Transforms / complete VRL coverage of traces, later helpers to manipulate traces or isolate outliers
- OpenTelemetry traces sink
- Add metrics then log to the `opentelemetry` source.

[otlp-dd-exporter]: https://github.com/open-telemetry/opentelemetry-collector-contrib/tree/64a87c1/exporter/datadogexporter
[otlp-traces-with-dd-agent]: https://docs.datadoghq.com/tracing/setup_overview/open_standards/#otlp-ingest-in-datadog-agent
[otlp-protocols]: https://github.com/open-telemetry/opentelemetry-specification/blob/main/specification/protocol/otlp.md
[otlp-proto-def]: https://github.com/open-telemetry/opentelemetry-proto/tree/main/opentelemetry/proto
[otlp-trace-proto-def]: https://github.com/open-telemetry/opentelemetry-proto/blob/main/opentelemetry/proto/trace/v1/trace.proto
[otlp-grpc-def]: https://github.com/open-telemetry/opentelemetry-proto/tree/main/opentelemetry/proto/collector
[otlp-http]: https://github.com/open-telemetry/opentelemetry-specification/blob/main/specification/protocol/otlp.md#otlphttp
[apm-stats-computation]: https://github.com/open-telemetry/opentelemetry-collector-contrib/blob/main/exporter/datadogexporter/stats.go#L30
[apm-stats-proto]: https://github.com/DataDog/datadog-agent/blob/main/pkg/trace/pb/stats.proto#L55-L70
[agent-code-for-otlp-exporter]: https://pkg.go.dev/github.com/DataDog/datadog-agent/pkg/trace/exportable@v0.0.0-20201016145401-4646cf596b02
[agent-handle-span]: https://github.com/DataDog/datadog-agent/blob/4646cf596b0242a7741328bd518a807b01db28c6/pkg/trace/exportable/stats/statsraw.go#L192
[dd-traces-proto]: https://github.com/DataDog/datadog-agent/tree/main/pkg/trace/pb
[span-conversion]: https://github.com/DataDog/datadog-agent/blob/882588c/pkg/trace/api/otlp.go#L320-L322
[schema-work]: https://github.com/vectordotdev/vector/issues/11300
[otlp-dd-trace-receiver]: https://github.com/open-telemetry/opentelemetry-collector-contrib/pull/5836
[dd-traces-to-otlm-code]: https://github.com/boostchicken/opentelemetry-collector-contrib/blob/2663e4de35eac5a06a194e8d6fb369318d9369fc/receiver/datadogreceiver/translator.go
[otlp-and-other-formats]: https://github.com/open-telemetry/opentelemetry-collector-contrib/blob/main/internal/coreinternal/tracetranslator/protospan_translation.go#L21-L31
[current-trace-in-vector]: https://github.com/vectordotdev/vector/blob/b6edb0203f684f67f8934da948cdf2bdd78d5236/lib/vector-core/src/event/trace.rs
[otlp-rust]: https://github.com/open-telemetry/opentelemetry-rust
[common-trace-format]: https://diamon.org/ctf/#specification
