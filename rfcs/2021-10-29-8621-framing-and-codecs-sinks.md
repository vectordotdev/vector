# RFC 8621 - 2021-10-29 - Framing and Codecs - Sinks

This RFC discusses changes to apply framing and encoding across sinks in a consistent way. On a high-level, this feature aims to be the symmetric counter part to the concept outlined [Framing and Codecs - Sources]([/blob/master/rfcs/2021-08-06-8619-framing-and-codecs-sources.md](https://github.com/vectordotdev/vector/blob/7796b3e766085225d2ebbe698a43d4015fe303c5/rfcs/2021-08-06-8619-framing-and-codecs-sources.md)).

## Context

In the context of sinks, we refer with _encoding_ to serializing an event to bytes and with _framing_ to the process of wrapping one or multiple serialized events into a bounded message that can be sent as a payload.

Currently, most sinks include the common `EncodingConfig<T>` in their config. It takes a generic argument where enums specify which encodings they support. However, the actual encoding logic is reimplemented by each sink individually, rather than falling back to a shared inventory of codec implementations. This leads to unnecessary drift in their feature set and behavior.

Furthermore, the shared `EncodingConfig` as it is implemented today is concerned with three tasks: Reshaping an event (including/excluding fields), serializing an event to a byte message, and framing/batching events. In accordance with the functionality that the decoding side provides, we want keep these concepts separate for a simpler mental model. In fact, we want to separate reshaping from codecs entirely and move it to the responsibilities of the schema work as outline in the next section.

## Cross cutting concerns

The codec work is very interweaved with the ongoing [schema work](https://github.com/vectordotdev/vector/pull/9388). It introduces the concept of "mappers" which understand how to transform an event at runtime based on the schema from its source and destination within Vector's topology.

We want to hand over the reshaping responsibilities to the schema, and provide a transition layer to support existing reshaping capabilities of sinks until schema mappers are implemented.

Another related current work is `StandardEncoding` which has been introduced in [#9215](https://github.com/vectordotdev/vector/pull/9215). It aims to solve one of our goals, by providing a consistent set of codec implementations. However, it does not separate the reshaping responsibilities.

## Scope

### In scope

- List work being directly addressed with this RFC.

### Out of scope

- List work that is completely out of scope. Use this to keep discussions focused. Please note the "future changes" section at the bottom.

## Pain

- What internal or external *pain* are we solving?
- Do not cover benefits of your change, this is covered in the "Rationale" section.

## Proposal

### User Experience

- Explain your change as if you were describing it to a Vector user. We should be able to share this section with a Vector user to solicit feedback.
- Does this change break backward compatibility? If so, what should users do to upgrade?

### Implementation

- Explain your change as if you were presenting it to the Vector team.
- When possible, demonstrate with psuedo code not text.
- Be specific. Be opinionated. Avoid ambiguity.

## Rationale

- Why is this change worth it?
- What is the impact of not doing this?
- How does this position us for success in the future?

## Drawbacks

- Why should we not do this?
- What kind on ongoing burden does this place on the team?

## Prior Art

- List prior art, the good and bad.
- Why can't we simply use or copy them?

## Alternatives

- What other approaches have been considered and why did you not choose them?
- How about not doing this at all?

## Outstanding Questions

- List any remaining questions.
- Use this to resolve ambiguity and collaborate with your team during the RFC process.
- *These must be resolved before the RFC can be merged.*

## Plan Of Attack

Incremental steps to execute this change. These will be converted to issues after the RFC is approved:

- [ ] Submit a PR with spike-level code _roughly_ demonstrating the change.
- [ ] Incremental change #1
- [ ] Incremental change #2
- [ ] ...

Note: This can be filled out during the review process.

## Future Improvements

- List any future improvements. Use this to keep your "plan of attack" scope small and project a sound design.

## Surveyed Sinks

Initial investigation for this RFC 

- aws_cloudwatch_logs: EncodingConfig, Encoding { Text, Json, }, enveloped in rusoto_logs::InputLogEvent. Text reads message_key()
- aws_kinesis_firehose: EncodingConfig, Encoding { Text, Json, }, enveloped in rusoto_firehose::Record that serializes to base64. Text reads message_key()
- aws_kinesis_streams: EncodingConfig, Encoding { Text, Json, }, enveloped in rusoto_kinesis::PutRecordsRequestEntry. Text reads message_key()
- aws_s3: EncodingConfig, StandardEncodings { Text, Json, Ndjson, }, uses util::{RequestBuilder, Encoder, Compressor}. Text reads message_key()
- aws_sqs: EncodingConfig, Encoding { Text, Json, }, enveloped in EncodedEvent<SendMessageEntry>. Text reads message_key()
- azure_blob: EncodingConfig, Encoding { Ndjson, Text, }, enveloped in EncodedEvent<PartitionInnerBuffer>. Text reads message_key()
- azure_monitor_logs: EncodingConfigWithDefault, Encoding { Default, }, only uses transformation capabilities
- blackhole: -/-
- clickhouse: EncodingConfigWithDefault, Encoding { Default, }, only uses transformation capabilities
- console: EncodingConfig, Encoding { Text, Json, }. Text reads message_key()
- datadog: EncodingConfigFixed, DatadogLogsJsonEncoding
- datadog_archives
- elasticsearch
- file: EncodingConfig, Encoding { Text, Ndjson, }. Text reads message_key()
- gcp
- honeycomb
- http: EncodingConfig, Encoding { Text, Ndjson, Json, }. Enveloped in HTTP request. Request-level compression. Sets headers depending on encoding
- humio
- influxdb
- kafka: EncodingConfig, StandardEncodings { Text, Json, Ndjson, }, doesn't apply transformations?, uses Encoder<Event> for StandardEncodings encode_input, enveloped in KafkaRequest
- logdna
- loki
- nats
- new_relic_logs
- papertrail
- pulsar
- redis
- sematext
- socket: EncodingConfig, Encoding { Text, Json, }. Text reads message_key()
- splunk_hec
- vector

