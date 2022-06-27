# RFC 8621 - 2021-10-29 - Framing and Codecs - Sinks

This RFC discusses changes to apply framing and encoding across sinks in a consistent way. On a high-level, this feature aims to be the symmetric counter part to the concept outlined [Framing and Codecs - Sources]([/blob/master/rfcs/2021-08-06-8619-framing-and-codecs-sources.md](https://github.com/vectordotdev/vector/blob/7796b3e766085225d2ebbe698a43d4015fe303c5/rfcs/2021-08-06-8619-framing-and-codecs-sources.md)).

## Context

In the context of sinks, we refer to _encoding_ as serializing an event to bytes, and to _framing_ as the process of wrapping one or more serialized events into a bounded message that can be used as the payload for a request sent by a sink.

Currently, most sinks include the common `EncodingConfig<T>` in their config. It takes a generic argument where enums specify which encodings they support. However, the actual encoding logic is reimplemented by each sink individually, rather than falling back to a shared inventory of codec implementations. This leads to unnecessary drift in their feature set and behavior.

Furthermore, the shared `EncodingConfig` as it is implemented today is concerned with three tasks: Reshaping an event (including/excluding fields), serializing an event to a byte message, and framing/batching events. In accordance with the functionality that the decoding side provides, we want to keep these concepts separate for a simpler mental model. In fact, we want to separate reshaping from codecs entirely and move it to the responsibilities of the schema work as outlined in the next section.

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

Users should be able to set `encoding` and `framing` options on sinks, analogously to the `decoding` and `framing` options on sources. These options uniformly control how the event _payload_ is encoded. This distinction is important, as encoding for the sink specific _protocol_ and the event _payload_ are separate concerns. The payload should still be encoded according to the sink's protocol, and the sink should provide additional options if there are multiple protocols to choose from, e.g. under a `protocol` key. If there need to be any encoding options for the payload at all can be decided on a per-sink basis.

The fields containing event transformations in sinks on the current `encoding` options (`only_fields`, `except_fields`, `timestamp_format`) should also work with the new encoding options without breaking backwards compatibility. In the future, they may be moved to a dedicated key, e.g. `transform` or replaced by a mechanism provided by the upcoming schema support.

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

These can be built and combined to form an `Encoder`:

```rust
/// An encoder that can encode structured events to byte messages.
pub struct Encoder {
    serializer: BoxedSerializer,
    framer: BoxedFramer,
}
```

`Encoder` implements `tokio_util::codec::Encoder<Event>`. Internally, events first go through the `Serializer` which implements `tokio_util::codec::Encoder<Event>` and are then handed over to the `Framer` which implements `tokio_util::codec::Encoder<()>`, such that serialized events can be framed in-place without additional allocations.

Sinks which don't need the framing capabilities of the `Encoder`, e.g. Kafka where the protocol is already message based, may only add a `SerializerConfig` to their config. The `Serializer` that can be built from the config implements `tokio_util::codec::Encoder<Event>` and therefore conforms to the same trait as `Encoder`.

## Plan Of Attack

Incremental steps to execute this change. These will be converted to issues after the RFC is approved:

- [ ] Implementation of support structures (`Encoder`, `SerializerConfig`, `FramingConfig`)
- [ ] Implementation of selected `encoders`/`framers` (e.g. `JSON` and `newline_delimited`)
- [ ] Implementation of compatibility layer for legacy reshaping via `EncodingConfiguration::apply_rules`
- [ ] Example integration with first sink, e.g. `socket` or `http` which benefit most from generic encoding/framing options
- [ ] Subsequent PRs for each integration to a sink

## Surveyed Sinks

Overview for the current state of sinks regarding encoding:

|sink|encoding config|`.apply_rules`|notes|
|-|-|-|-|
|`aws_cloudwatch_logs`| `EncodingConfig<Encoding { Text, Json }>` | ✔︎ | Enveloped in `rusoto_logs::InputLogEvent`. `Text` reads message_key()
|`aws_kinesis_firehose`| `EncodingConfig<Encoding { Text, Json }>` | ✔︎ | Enveloped in `rusoto_firehose::Record` that serializes to base64. `Text` reads `message_key()` | -
|`aws_kinesis_streams`| `EncodingConfig<Encoding { Text, Json }>` | ✔︎ | Enveloped in `rusoto_kinesis::PutRecordsRequestEntry`. `Text` reads `message_key()`
|`aws_s3`| `EncodingConfig<StandardEncodings { Text, Json, Ndjson }>` | ✔︎ | Uses `util::{RequestBuilder, Encoder, Compressor}`. `Text` reads `message_key()`
|`aws_sqs`| `EncodingConfig<Encoding { Text, Json }>` | ✔︎ | Enveloped in `EncodedEvent<SendMessageEntry>`. `Text` reads `message_key()`
|`azure_blob`| `EncodingConfig<Encoding { Ndjson, Text }>` | ✔︎ | Enveloped in `EncodedEvent<PartitionInnerBuffer>`. `Text` reads `message_key()`
|`azure_monitor_logs`| `EncodingConfigWithDefault<Encoding { Default }>` | ✔︎ | Serializes to JSON. Enveloped in HTTP request
|`blackhole`| - | - | -
|`clickhouse`| `EncodingConfigWithDefault<Encoding { Default }>` | ✔︎ | Serializes to JSON. Enveloped in HTTP request
|`console`| `EncodingConfig<Encoding { Text, Json }>` | ✔︎ | `Text` reads `message_key()`
|`datadog_logs`| `EncodingConfigFixed<DatadogLogsJsonEncoding>` | ✔︎ | Doesn't provide options to encode the event payload separately from the protocol
|`datadog_events`| - | ✗ | -
|`datadog_archives`| - | ✗ | Uses custom `DatadogArchivesEncoding`, which has a field `inner: StandardEncodings` which is not user-configurable
|`elasticsearch`| `EncodingConfigFixed<ElasticsearchEncoder>` | ✔︎ | Reshapes event internally and implements custom `ElasticsearchEncoder` to serialize for the Elasticsearch protocol
|`file`| `EncodingConfig<Encoding { Text, Ndjson }>` | ✔︎ | `Text` reads `message_key()`
|`gcp`| `EncodingConfig<StandardEncodings>` | ✔︎ | Enveloped in HTTP request via `sinks::util::request_builder::RequestBuild`. Sets HTTP request header depending on encoding config
|`honeycomb`| - | ✗ | Embeds event as JSON under a `data` key. Enveloped in HTTP request
|`http`| `EncodingConfig<Encoding { Text, Ndjson, Json }>` | ✔︎ | Enveloped in HTTP request. Request-level compression. Sets HTTP request header depending on encoding config
|`humio`| `EncodingConfig<Encoding { Json, Text }>` | ✔︎ | Wrapper, see `splunk_hec` for more information
|`influxdb`| `EncodingConfigWithDefault<Encoding { Default }>` | ✔︎ | Encoding (for the protocol envelope) is done by routing event fields either into a "tags" or "fields" map that are passed into an internal function `influx_line_protocol`
|`kafka`| `EncodingConfig<StandardEncodings { Text, Json, Ndjson }>` | ✔︎ | Uses `Encoder<Event>` for `StandardEncodings` in `encode_input`, enveloped in `KafkaRequest`
|`logdna`| `EncodingConfigWithDefault<Encoding { Default }>` | ✔︎ | Builds a message by manually picking fields from the event. Enveloped in HTTP request
|`loki`| `EncodingConfig<Encoding { Json, Text, Logfmt }>` | ✔︎ | Uses reshaping. `Text` reads `message_key()`, `Logfmt` build a key-value string. Sink has config to preprocess event by adding/removing label fields and timestamp. Enveloped in HTTP request
|`nats`| `EncodingConfig<Encoding { Text, Json }>` | ✔︎ | `Text` reads `message_key()`
|`new_relic_logs`| `EncodingConfigWithDefault<Encoding { Json }>` | ✔︎ | Defers to HTTP sink, uses encoding config to reshape only and convert to JSON
|`papertrail`| `EncodingConfig<Encoding { Text, Json }>` | ✔︎ | `Text` reads `message_key()`. Serializes event using syslog and sends buffer via TCP
|`pulsar`| `EncodingConfig<Encoding { Text, Json, Avro }>` | ✔︎ | `Text` reads `message_key()`, `Avro` expects another dedicated key for the serialization schema. Serialized buffer is sent to Pulsar producer
|`redis`| `EncodingConfig<Encoding { Text, Json }>` | ✔︎ | `Text` reads `message_key()`. Encoded message is serialized to buffer
|`sematext`| `EncodingConfigFixed<ElasticsearchEncoder>` | ✔︎ | Wrapper, see `elasticsearch` for more information
|`socket`| `EncodingConfig<Encoding { Text, Json }>` | ✔︎ | `Text` reads `message_key()`
|`splunk_hec`| `EncodingConfig<HecLogsEncoder { Json, Text }>` | ✔︎ | Encoding is used to create a message according to the Splunk HEC protocol. There is no separate control over encoding the payload itself
|`vector`| - | - | -
