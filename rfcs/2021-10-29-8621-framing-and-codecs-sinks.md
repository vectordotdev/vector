# RFC 8621 - 2021-10-29 - Framing and Codecs - Sinks

This RFC discusses changes to apply framing and encoding across sinks in a consistent way. On a high-level, this feature aims to be the symmetric counter part to the concept outlined [Framing and Codecs - Sources]([/blob/master/rfcs/2021-08-06-8619-framing-and-codecs-sources.md](https://github.com/vectordotdev/vector/blob/7796b3e766085225d2ebbe698a43d4015fe303c5/rfcs/2021-08-06-8619-framing-and-codecs-sources.md)).

## Context

In the context of sinks, we refer to _encoding_ as serializing an event to bytes, and to _framing_ as the process of wrapping one or more serialized events into a bounded message that can be used as the payload for a request sent by a sink.

Currently, most sinks include the common `EncodingConfig<T>` in their config. It takes a generic argument where enums specify which encodings they support. However, the actual encoding logic is reimplemented by each sink individually, rather than falling back to a shared inventory of codec implementations. This leads to unnecessary drift in their feature set and behavior.

Furthermore, the shared `EncodingConfig` as it is implemented today is concerned with three tasks: Reshaping an event (including/excluding fields), serializing an event to a byte message, and framing/batching events. In accordance with the functionality that the decoding side provides, we want keep these concepts separate for a simpler mental model. In fact, we want to separate reshaping from codecs entirely and move it to the responsibilities of the schema work as outline in the next section.

## Cross cutting concerns

The codec work is very interweaved with the ongoing [schema work](https://github.com/vectordotdev/vector/pull/9388). It introduces the concept of "mappers" which understand how to transform an event at runtime based on the schema from its source and destination within Vector's topology.

We want to hand over the reshaping responsibilities to the schema, and provide a transition layer to support existing reshaping capabilities of sinks until schema mappers are implemented.

Another related change is `StandardEncodings` which was introduced in [#9215](https://github.com/vectordotdev/vector/pull/9215). It provides a consistent implementation of common codecs, and therefore aligns with our goals. However, it does not separate the reshaping responsibilities.

## Scope

### In scope

- Common support structures for encoding/framing
- Set of commonly used encoding (text, JSON, ...) and framing (newline-delimited, octet-counting, ...) methods
- Strategy for integrating encoding/framing with sinks

### Out of scope

- Integration with schema system (automatic transformation of event fields depending)

## Pain

Internally, we don't share implementations for encoding strategies that are very commonly used (e.g. JSON, newline-delimited). This leads to duplicated effort and missing guidance for handling encoding in sinks since no canonical implementation exists.

Externally, users are restricted by the available custom set of encodings implemented by the sink. There does not exist any mental model for consistent encoding/framing options for event data that applies to all sinks.

## Proposal

### User Experience

Users should be able to set `encoding` and `framing` options on sinks, analogously to the `decoding` and `framing` options on sources. These options uniformly control how the event _payload_ is encoded. This distinction is important, as encoding for the sink specific _protocol_ and the event _payload_ are separate concerns. The payload should still be encoded according to the sink's protocol, and the sink should provide additional options if there are multiple protocols to choose from, e.g. under a `protocol` key.

The fields containing event transformations in sinks on the current `encoding` options (`schema`, `only_fields`, `except_fields`, `timestamp_format`) should be moved to a dedicated option, e.g. `schema` or `transform`. Thus, this would introduce a breaking change in configurations. However, migration would be relatively straight-forward by nesting these options under the new key.

### Implementation

The following common config structures will be provided, analogously to the decoding/framing configs:

```rust
pub trait SerializerConfig: Debug + DynClone + Send + Sync {
    /// Builds a serializer from this configuration.
    ///
    /// Fails if the configuration is invalid.
    fn build(&self) -> crate::Result<BoxedSerializer>;
}
```

```rust
pub trait FramingConfig: Debug + DynClone + Send + Sync {
    /// Builds a framer from this configuration.
    ///
    /// Fails if the configuration is invalid.
    fn build(&self) -> crate::Result<BoxedFramer>;
}
```

These can be build to form an `Encoder`:

```rust
/// An encoder that can encode structured events to byte messages.
pub struct Encoder {
    serializer: BoxedSerializer,
    framer: BoxedFramer,
}
```

`Encoder` implements `tokio_util::codec::Encoder<SmallVec<[Event; 1]>>`. Internally, events first go through the `Serializer` which implements `tokio_util::codec::Encoder<SmallVec<[Event; 1]>>` and are then handed over to the `Framer` which implements `tokio_util::codec::Encoder<Bytes>`.

## Plan Of Attack

Incremental steps to execute this change. These will be converted to issues after the RFC is approved:

- [ ] Implementation of support structures (`Encoder`, `SerializerConfig`, `FramingConfig`)
- [ ] Implementation of selected `encoders`/`framers` (e.g. `JSON` and `newline_delimited`)
- [ ] Example integration with first sink, e.g. `socket` or `http` which benefit most from generic encoding/framing options
- [ ] Subsequent PRs for each integration to a sink

## Surveyed Sinks

Overview for the current state of sinks regarding encoding:

|sink|encoding config|notes|
|-|-|-|
|`aws_cloudwatch_logs`| `EncodingConfig<Encoding { Text, Json }>` | Enveloped in `rusoto_logs::InputLogEvent`. `Text` reads message_key()
|`aws_kinesis_firehose`| `EncodingConfig<Encoding { Text, Json }>` | Enveloped in `rusoto_firehose::Record` that serializes to base64. `Text` reads `message_key()`
|`aws_kinesis_streams`| `EncodingConfig<Encoding { Text, Json }>` | Enveloped in `rusoto_kinesis::PutRecordsRequestEntry`. `Text` reads `message_key()`
|`aws_s3`| `EncodingConfig<StandardEncodings { Text, Json, Ndjson }>` | Uses util::{RequestBuilder, Encoder, Compressor}. `Text` reads `message_key()`
|`aws_sqs`| `EncodingConfig<Encoding { Text, Json }>` | Enveloped in EncodedEvent<SendMessageEntry>. `Text` reads `message_key()`
|`azure_blob`| `EncodingConfig<Encoding { Ndjson, Text }>` | Enveloped in EncodedEvent<PartitionInnerBuffer>. `Text` reads `message_key()`
|`azure_monitor_logs`| `EncodingConfigWithDefault<Encoding { Default }>` | Reshapes events only, without encoding
|`blackhole`| -/-
|`clickhouse`| `EncodingConfigWithDefault<Encoding { Default }>` | Reshapes events only, without encoding
|`console`| `EncodingConfig<Encoding { Text, Json }>` | `Text` reads `message_key()`
|`datadog`| `EncodingConfigFixed<DatadogLogsJsonEncoding>` | Doesn't provide options to encode the event payload separately from the protocol
|`datadog_archives`|
|`elasticsearch`|
|`file`| `EncodingConfig<Encoding { Text, Ndjson }>` | `Text` reads `message_key()`
|`gcp`|
|`honeycomb`|
|`http`| `EncodingConfig<Encoding { Text, Ndjson, Json }>` | Enveloped in HTTP request. Request-level compression. Sets headers depending on encoding config
|`humio`|
|`influxdb`|
|`kafka`| `EncodingConfig<StandardEncodings { Text, Json, Ndjson }>` | Doesn't reshape, uses `Encoder<Event>` for `StandardEncodings` in `encode_input`, enveloped in `KafkaRequest`
|`logdna`|
|`loki`|
|`nats`|
|`new_relic_logs`|
|`papertrail`|
|`pulsar`|
|`redis`|
|`sematext`|
|`socket`| `EncodingConfig<Encoding { Text, Json }>` | `Text` reads `message_key()`
|`splunk_hec`|
|`vector`|
