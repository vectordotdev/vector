# RFC 9930 - 2021-11-05 - Native event encoding

While Vector has the ability to decode and encode data from a variety of formats such as plaintext
or JSON, it currently lacks the ability to encode its internal event representation to a portable
format that can be used with existing sources and sinks other than `vector`.  We propose adding new
codecs to allow just that.

## Context

- [Standardized metric encoding](https://github.com/vectordotdev/vector/issues/8494)
- [Make vector less opinionated: allow customization of the "shape" of expected/outputted events for "generic" sources/sinks](https://github.com/vectordotdev/vector/issues/9347)
- [Use Kafka as a buffer between two Vector instances (decoding Vector data)](https://github.com/vectordotdev/vector/issues/5809)
- [Send metrics data to Prometheus through Kafka](https://github.com/vectordotdev/vector/issues/9186)
- [Datadog codec](https://github.com/vectordotdev/vector/issues/7109)
- [Allow metrics to be sent and received with http source and sink](https://github.com/vectordotdev/vector/issues/6825)
- [Decoders for sources](https://github.com/vectordotdev/vector/issues/257)

## Cross cutting concerns

- This would overlap with the tailored [Buffer Improvements](https://github.com/vectordotdev/vector/issues/9476)
  work happening in 2021Q4, as native event encoding/decoding could also be used to emulate
  buffering between Vector processes.

## Scope

### In scope

- Allowing users to push data into Vector (via a standard source) that can be decoded natively into
  the internal Vector event representation.
- Handling logs, metrics, and eventually traces, both for decoding and encoding.
- Providing a simple schema that developers could reference if manually generating payloads outside
  of Vector itself.

### Out of scope

- Implementing it for a specific source/sink pair.
- Forward/backwards compatibility guarantees for anything except for Protocol Buffers.
- Handling arbitrary native formats in differing sources/sinks i.e. letting a statsd source parse the
  Prometheus exposition format, or having the Kafka sink spit out metrics in the Influx line
  protocol, etc.
- Versioned schemas for formats that are not already implicitly versionable. (i.e. no versioned JSON schema)

## Pain

Users routinely use Vector as a unifying step in their observability pipeline: taking disparate
sources and transforming, filtering, and cleaning up that data before sending it off to downstream
systems.  This means that often times, Vector may not support the type of data they want to send,
and there's a required step of adapting their data to use with Vector.  This isn't a problem that
can be entirely solved, but one that is currently harder for users to solve than it should be.

Sources like [`exec`](https://github.com/vectordotdev/vector/issues/992) were borne out of a desire
to let users arbitrarily feed data into Vector from a simple shell script or process, which itself
could trivially pull and generate whatever data was desired.  However, there are still limitations
due to the fact that users must do subsequent transformation steps to extract metrics from log lines,
and so on.

As well, users are constrained when they want to send data from one Vector instance to another by
Vector only supporting this via the native gRPC-based `vector` source and sink.  If users already
had a blessed solution for service-to-service data flow, such as Kafka, they would be stuck using the
aforementioned transformation steps to go back and forth between available encoded formats and back
into the desired metric types within Vector.

## Proposal

### User Experience

Users would be able to specify two new encoding types for supported sources and sinks, called
`vector_native` and `vector_json`, that would encode and decode the data natively into the internal
Vector `Event` type from both Protocol Buffers and JSON, respectively.

The `vector_native` codec uses Protocol Buffers and mirrors the codec used by the `vector` source
and sink.  This codec follows our public Protocol Buffers definition in the repository, and is
treated as a tier one schema: we commit to not updating the protocol in backwards or
forwards-incompatible ways.

The `vector_json` codec uses JSON and would generally mirror the internal structure of a Vector
event flowing through the system.  This codec has minimal support for versioning, and is subject to
change as the internal representation of Vector events evolves over time.  A human-readable schema would
be generated as part of builds/releases, and would be mentioned in upgrade guides when there is a
breaking change, but we would generally only commit to interoperability between Vector instances
running the same version.

### Implementation

- The `vector_native` codec would be based on the same exact Protocol Buffers definition we use for
  the `vector` source and sink.
- The `vector_json` codec would be based on using `serde` to serialize `Event` to JSON.
- Both `LogEvent` and `Metric` derive a `serde::Deserialize` implementation already, while `Metric`
  also derives a `serde::Serialize` implementation.
- We would add missing `serde` derives to `LogEvent` and `Event` itself, allowing top-level `Event`s
  to be trivially serialized and deserialized.
- We would _not_ serialize/deserialize event metadata, which currently only includes event
  finalizers and a Datadog API key override field.
- Event metadata may come into scope in the future where there is a more generalized mechanism for
  adding metadata to events, but we would need to design a mechanism to filter "internal" metadata
  vs "external" metadata, as we would not want to push API keys in plaintext, etc.
- The existing framing/codec work happening for both sources and sinks would gain two new
  implementations for `vector_native` and `vector_json`, respectively.
- We would use `serde-reflection` to generate a basic schema of `Event`, which could be stored in
  the source code itself, similar in principle to `Cargo.lock`.  This would serve as the minimum
  viable schema for JSON use cases, without any commitment to versioning or backwards/forwards-compatibility.

## Rationale

Adding encodings for natively representing events would provide an additional avenue for users to
both ingest data into Vector, as well as constructing more complex Vector deployment topologies.  As
Vector development can often be bottlenecked when it comes to adding new sources and sink, this work
would act as a force multiplier for letting users invest a small amount of time converting their
data to the native format, and then being able to universally ingest it.

If we didn't do this, it would not necessarily hurt the long-term goals of Vector, but it would
require more effort over time in order to develop new sources and sinks to meet the demands of users
who wish to use Vector with systems we don't already support.  This could hurt the long-term
_success_ of Vector.

## Drawbacks

Encoding `Event` natively via Protocol Buffers should be a feature we can accomplish with no
additional burden on the Vector team, as we already perform the necessary due diligence and spend
time ensuring that our Protocol Buffers definition stays backwards/forwards-compatible.

Encoding `Event` to JSON, however, could result in more time spent by the Vector team on support to
the lack of a stringent schema, given that we would not be transforming `Event` to a known
definition like Protocol Buffers.  While the idea to generate a minimal viable schema could
hopefully alleviate some of those concerns, it still doesn't address the notion of not providing
versioned JSON schemas or backwards/forwards-compatibility, which would likely represent the bulk of
issues brought to us for users utilizing `vector_json`.

## Prior Art

Most of the relevant prior art would be related to our existing usage of Protocol Buffers for
Vector-to-Vector communication via the `vector` source and sink.

Additionally, there is an existing/draft standard for `JSON Schema`, a schema for JSON.  This would
be a more robust schema to provide users compared to what `serde-reflection` can generate.  However,
it still does not deal with versioning of the schema, or make it any easier to do
backwards/forwards-compatible changes to the schema.

At a higher level, of the typical alternatives to Vector, Cribl appears to be the only solution
where their TCP JSON source can accept an arbitrary JSON payload that allows setting what Cribl
calls "internal" fields. However, these fields are "used only within Cribl LogStream, and are not
passed down to Destinations"[1] and so this does not appear to be a generic solution comparable to
what this RFC proposes.

## Alternatives

We could continue to push the `vector` source and sink as the supported method of handling
Vector-to-Vector communication.  In practice, users seem to be fine with utilizing this approach,
and it is the basis of our Kubernetes-based aggregator deployment pattern.  Practically speaking,
any environment which utilizes another technology as their primary mechanism for service-to-service
data flow could technically allow the communication flows to allow the `vector` source and sink to
be used.  However, this does not address the potential desire for Vector to fit into an existing
infrastructure, rather than the other way around.

Additionally, we could also provide more specific codecs designed around specific protocols --
Prometheus exposition format, Influx line protocol, etc -- and allow those to be configured at a
source and sink level.  This requires users to add additional source/sink pipelines to their
configurations to handle those event types specifically.  This could mean dedicated Kafka topics, or
HTTP endpoints, and so on, depending on the event type/codec chosen, instead of the "universal"
format provided by shipping `Event`s natively.

## Outstanding Questions

- Is there a better format than JSON that we could/should use as the human-readable variant?

## Plan Of Attack

Incremental steps to execute this change. These will be converted to issues after the RFC is approved:

- [ ] Implement the remaining `serde` derives on `Event` and `LogEvent`, including event metadata
  exclusion.
- [ ] Add support for both `vector_native` and `vector_json` to the existing framing/decoding
  infrastructure used for sources.
- [ ] Add support for both `vector_native` and `vector_json` to `StandardEncodings` to provide the
  sink-side support, or to the framing/encoding infrastructure if it supports sinks by then.
- [ ] Use `serde-reflection` to generate a minimum viable schema definition that can be added to the
  repository, potentially as a Vector subcommand so Vector binaries can be self-documenting.

## Future Improvements

- Adding a CI step that runs the same Vector subcommand (or whatever approach we use for running
  `serde-reflection`) and compares it to whats currently in the repository, to ensure we don't let
  the schema get out-of-sync from the actual `serde` output.
