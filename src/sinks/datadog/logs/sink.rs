use std::{collections::VecDeque, fmt::Debug, io, sync::Arc};

use bytes::Bytes;
use itertools::Itertools;
use snafu::Snafu;
use tracing::Instrument;
use vector_lib::{
    event::{ObjectMap, Value},
    internal_event::{ComponentEventsDropped, UNINTENTIONAL},
    lookup::event_path,
};
use vrl::path::{OwnedSegment, OwnedTargetPath, PathPrefix};

use super::{config::DatadogLogsTruncationConfig, service::LogApiRequest};
use crate::{
    common::datadog::{DD_RESERVED_SEMANTIC_ATTRS, DDTAGS, MESSAGE, is_reserved_attribute},
    internal_events::{DatadogLogsEventTruncated, DatadogLogsReservedAttributeConflict},
    sinks::{
        prelude::*,
        util::{Compressor, http::HttpJsonBatchSizer},
    },
};
#[derive(Default)]
pub struct EventPartitioner;

impl Partitioner for EventPartitioner {
    type Item = Event;
    type Key = Option<Arc<str>>;

    fn partition(&self, item: &Self::Item) -> Self::Key {
        item.metadata().datadog_api_key()
    }
}

#[derive(Debug)]
pub struct LogSinkBuilder<S> {
    transformer: Transformer,
    service: S,
    batch_settings: BatcherSettings,
    compression: Option<Compression>,
    default_api_key: Arc<str>,
    protocol: String,
    conforms_as_agent: bool,
    max_payload_bytes: usize,
    truncation: Option<DatadogLogsTruncationConfig>,
}

impl<S> LogSinkBuilder<S> {
    pub const fn new(
        transformer: Transformer,
        service: S,
        default_api_key: Arc<str>,
        batch_settings: BatcherSettings,
        protocol: String,
        conforms_as_agent: bool,
        max_payload_bytes: usize,
    ) -> Self {
        Self {
            transformer,
            service,
            default_api_key,
            batch_settings,
            compression: None,
            protocol,
            conforms_as_agent,
            max_payload_bytes,
            truncation: None,
        }
    }

    pub const fn compression(mut self, compression: Compression) -> Self {
        self.compression = Some(compression);
        self
    }

    pub const fn truncation(mut self, truncation: Option<DatadogLogsTruncationConfig>) -> Self {
        self.truncation = truncation;
        self
    }

    pub fn build(self) -> LogSink<S> {
        LogSink {
            default_api_key: self.default_api_key,
            transformer: self.transformer,
            service: self.service,
            batch_settings: self.batch_settings,
            compression: self.compression.unwrap_or_default(),
            protocol: self.protocol,
            conforms_as_agent: self.conforms_as_agent,
            max_payload_bytes: self.max_payload_bytes,
            truncation: self.truncation,
        }
    }
}

pub struct LogSink<S> {
    /// The default Datadog API key to use
    ///
    /// In some instances an `Event` will come in on the stream with an
    /// associated API key. That API key is the one it'll get batched up by but
    /// otherwise we will see `Event` instances with no associated key. In that
    /// case we batch them by this default.
    default_api_key: Arc<str>,
    /// The API service
    service: S,
    /// The encoding of payloads
    transformer: Transformer,
    /// The compression technique to use when building the request body
    compression: Compression,
    /// Batch settings: timeout, max events, max bytes, etc.
    batch_settings: BatcherSettings,
    /// The protocol name
    protocol: String,
    /// Normalize events to agent standard and attach associated HTTP header to request
    conforms_as_agent: bool,
    /// Maximum uncompressed payload size in bytes
    max_payload_bytes: usize,
    /// Limits used when oversized log truncation is enabled.
    truncation: Option<DatadogLogsTruncationConfig>,
}

// The Datadog logs intake does not require the fields that are set in this
// function. But if they are present in the event, we normalize the paths
// (and value in the case of timestamp) to something that intake understands.
pub fn normalize_event(event: &mut Event) {
    let log = event.as_mut_log();

    // Will cast the internal value to an object if it already isn't
    if !log.value().is_object() {
        log.insert(event_path!(MESSAGE), log.value().clone());
    }

    // Upstream Sources may have semantically defined Datadog reserved attributes outside of their
    // expected location by DD logs intake (root of the event). Move them if needed.
    for (meaning, expected_field_name) in DD_RESERVED_SEMANTIC_ATTRS {
        // check if there is a semantic meaning for the reserved attribute
        if let Some(current_path) = log.find_key_by_meaning(meaning).cloned() {
            // move it to the desired location
            position_reserved_attr_event_root(log, &current_path, expected_field_name, meaning);
        }
    }

    // if the tags value is an array we need to reconstruct it to a comma delimited string for DD logs intake.
    // NOTE: we don't access by semantic meaning here because in the prior step
    // we ensured reserved attributes are in expected locations.
    let ddtags_path = event_path!(DDTAGS);
    if let Some(Value::Array(tags_arr)) = log.get(ddtags_path)
        && !tags_arr.is_empty()
    {
        let all_tags: String = tags_arr
            .iter()
            .filter_map(|tag_kv| {
                tag_kv
                    .as_bytes()
                    .map(|bytes| String::from_utf8_lossy(bytes))
            })
            .join(",");

        log.insert(ddtags_path, all_tags);
    }

    // ensure the timestamp is in expected format
    // NOTE: we don't access by semantic meaning here because in the prior step
    // we ensured reserved attributes are in expected locations.
    let ts_path = event_path!("timestamp");
    if let Some(Value::Timestamp(ts)) = log.remove(ts_path) {
        log.insert(ts_path, Value::Integer(ts.timestamp_millis()));
    }
}

// Optionally for all other non-reserved fields, nest these under the `message` key. This is the
// final step in having the event conform to the standard that the logs intake expects when an
// event originates from an agent. Normalizing the events to the format prepared by the datadog
// agent resolves any inconsistencies that would be observed when data flows through vector
// before being ingested by the logs intake. This is because the logs intake interprets the
// request with slight differences when this header and format are observed.
pub fn normalize_as_agent_event(event: &mut Event) {
    let log = event.as_mut_log();
    // Should never occur since normalize_event forces a conversion of the log value to an Object type
    let Some(object_map) = log.as_map_mut() else {
        return;
    };
    // Move all non reserved fields into a new object
    let mut local_root = ObjectMap::default();
    let keys_to_move = object_map
        .keys()
        .filter(|ks| !is_reserved_attribute(ks.as_str()))
        .cloned()
        .collect::<Vec<_>>();
    for key in keys_to_move {
        if let Some((entry_k, entry_v)) = object_map.remove_entry(key.as_str()) {
            local_root.insert(entry_k, entry_v);
        }
    }
    // .. nest this object at the root under the reserved key named 'message'
    log.insert(event_path!(MESSAGE), local_root);
}

// If an expected reserved attribute is not located in the event root, rename it and handle
// any potential conflicts by preserving the conflicting one with a _RESERVED_ prefix.
pub fn position_reserved_attr_event_root(
    log: &mut LogEvent,
    current_path: &OwnedTargetPath,
    expected_field_name: &str,
    meaning: &'static str,
) {
    // the path that DD archives expects this reserved attribute to be in.
    let desired_path = event_path!(expected_field_name);

    // if not already be at the expected location
    if !path_is_field(current_path, expected_field_name) {
        // if an existing attribute exists here already, move it so to not overwrite it.
        // yes, technically the rename path could exist, but technically that could always be the case.
        if log.contains(desired_path) {
            let rename_attr = format!("_RESERVED_{meaning}");
            let rename_path = event_path!(rename_attr.as_str());
            emit!(DatadogLogsReservedAttributeConflict {
                meaning,
                source_path: current_path,
                destination_path: expected_field_name,
                renamed_existing_to: &rename_attr,
            });
            log.rename_key(desired_path, rename_path);
        }

        log.rename_key(current_path, desired_path);
    }
}

// Test if the named path consists of the single named field. This is rather a hack and should
// hypothetically be solvable in the `vrl` crate with an implementation of
// `PartialEq<BorrowedTargetPath<'_>>`. The alternative is doing a comparison against another
// `OwnedTargetPath`, but the naïve implementation of that requires multiple allocations and copies
// just to test equality.
pub fn path_is_field(path: &OwnedTargetPath, field: &str) -> bool {
    path.prefix == PathPrefix::Event
        && matches!(&path.path.segments[..], [OwnedSegment::Field(f)] if f.as_str() == field)
}

#[derive(Debug, Snafu)]
pub enum RequestBuildError {
    #[snafu(display("Encoded payload is greater than the max limit."))]
    PayloadTooBig { events_that_fit: usize },
    #[snafu(display("Failed to build payload with error: {}", error))]
    Io { error: std::io::Error },
    #[snafu(display("Failed to serialize payload with error: {}", error))]
    Json { error: serde_json::Error },
}

impl From<io::Error> for RequestBuildError {
    fn from(error: io::Error) -> RequestBuildError {
        RequestBuildError::Io { error }
    }
}

impl From<serde_json::Error> for RequestBuildError {
    fn from(error: serde_json::Error) -> RequestBuildError {
        RequestBuildError::Json { error }
    }
}

struct LogRequestBuilder {
    pub default_api_key: Arc<str>,
    pub transformer: Transformer,
    pub compression: Compression,
    pub conforms_as_agent: bool,
    pub max_payload_bytes: usize,
    pub truncation: Option<DatadogLogsTruncationConfig>,
}

impl LogRequestBuilder {
    pub fn build_request(
        &self,
        events: Vec<Event>,
        api_key: Arc<str>,
    ) -> Result<Vec<LogApiRequest>, RequestBuildError> {
        // Transform events and pre-compute their estimated size.
        let mut events_with_estimated_size: VecDeque<(Event, JsonSize)> = events
            .into_iter()
            .map(|mut event| {
                normalize_event(&mut event);
                if self.conforms_as_agent {
                    normalize_as_agent_event(&mut event);
                }
                self.transformer.transform(&mut event);
                let estimated_json_size = event.estimated_json_encoded_size_of();
                (event, estimated_json_size)
            })
            .collect();

        // Construct requests respecting the max payload size.
        let mut requests: Vec<LogApiRequest> = Vec::new();
        while !events_with_estimated_size.is_empty() {
            let (events_serialized, body, byte_size) =
                self.serialize_with_capacity(&mut events_with_estimated_size)?;
            if events_serialized.is_empty() {
                if events_with_estimated_size.pop_front().is_some() {
                    emit!(ComponentEventsDropped::<UNINTENTIONAL> {
                        count: 1,
                        reason: "Event too large to encode."
                    });
                }
                continue;
            }
            let request =
                self.finish_request(body, events_serialized, byte_size, Arc::clone(&api_key))?;
            requests.push(request);
        }

        Ok(requests)
    }

    fn serialize_with_capacity(
        &self,
        events: &mut VecDeque<(Event, JsonSize)>,
    ) -> Result<(Vec<Event>, Vec<u8>, GroupedCountByteSize), io::Error> {
        let total_estimated =
            events.iter().map(|(_, size)| size.get()).sum::<usize>() + events.len() * 2;
        let mut buf = Vec::with_capacity(total_estimated);
        let mut byte_size = telemetry().create_request_count_byte_size();
        let mut events_serialized = Vec::with_capacity(events.len());

        buf.push(b'[');
        while let Some((mut event, mut estimated_json_size)) = events.pop_front() {
            let existing_len = buf.len();
            match encode_log(
                &mut buf,
                &mut event,
                !events_serialized.is_empty(),
                self.truncation,
                self.conforms_as_agent,
            )? {
                LogEncoding::Unchanged => {}
                LogEncoding::Truncated => {
                    estimated_json_size = event.estimated_json_encoded_size_of();
                }
                LogEncoding::Dropped { reason } => {
                    emit!(ComponentEventsDropped::<UNINTENTIONAL> { count: 1, reason });
                    continue;
                }
            }

            if buf.len() >= self.max_payload_bytes {
                events.push_front((event, estimated_json_size));
                buf.truncate(existing_len);
                break;
            }
            byte_size.add_event(&event, estimated_json_size);
            events_serialized.push(event);
        }
        buf.push(b']');

        Ok((events_serialized, buf, byte_size))
    }

    fn finish_request(
        &self,
        buf: Vec<u8>,
        mut events: Vec<Event>,
        byte_size: GroupedCountByteSize,
        api_key: Arc<str>,
    ) -> Result<LogApiRequest, RequestBuildError> {
        let n_events = events.len();
        let uncompressed_size = buf.len();

        // Now just compress it like normal.
        let mut compressor = Compressor::from(self.compression);
        write_all(&mut compressor, n_events, &buf)?;
        let bytes = compressor.into_inner().freeze();

        let finalizers = events.take_finalizers();
        let request_metadata_builder = RequestMetadataBuilder::from_events(&events);

        let payload = if self.compression.is_compressed() {
            EncodeResult::compressed(bytes, uncompressed_size, byte_size)
        } else {
            EncodeResult::uncompressed(bytes, byte_size)
        };

        Ok::<_, RequestBuildError>(LogApiRequest {
            api_key,
            finalizers,
            compression: self.compression,
            metadata: request_metadata_builder.build(&payload),
            uncompressed_size: payload.uncompressed_byte_size,
            body: payload.into_payload(),
        })
    }
}

enum LogEncoding {
    Unchanged,
    Truncated,
    Dropped { reason: &'static str },
}

fn encode_log(
    buf: &mut Vec<u8>,
    event: &mut Event,
    include_comma: bool,
    truncation: Option<DatadogLogsTruncationConfig>,
    conforms_as_agent: bool,
) -> Result<LogEncoding, io::Error> {
    let existing_len = buf.len();
    let original_encoded_size = write_log(buf, event, include_comma)?;
    let Some(truncation) =
        truncation.filter(|truncation| original_encoded_size > truncation.max_log_bytes)
    else {
        return Ok(LogEncoding::Unchanged);
    };

    let Some(message) = message_bytes_mut(event.as_mut_log()).map(|message| message.clone()) else {
        buf.truncate(existing_len);
        return Ok(LogEncoding::Dropped {
            reason: "Oversized event has no string message to truncate.",
        });
    };
    let mut body_len =
        floor_char_boundary(&message, message.len().min(truncation.max_message_bytes));
    if body_len < message.len() {
        set_truncated_message(event.as_mut_log(), &message, body_len);
    }
    ensure_truncated_tag(event.as_mut_log());

    let mut encoded_size = rewrite_log(buf, existing_len, event, include_comma)?;
    if encoded_size > truncation.max_log_bytes {
        strip_non_standard_fields(event.as_mut_log(), conforms_as_agent);
        encoded_size = rewrite_log(buf, existing_len, event, include_comma)?;
    }

    if encoded_size > truncation.max_log_bytes {
        let marker_bytes_to_add = TRUNCATION_MARKER.len() * usize::from(body_len == message.len());
        let bytes_to_remove = (encoded_size - truncation.max_log_bytes) + marker_bytes_to_add;
        body_len = floor_char_boundary(&message, body_len.saturating_sub(bytes_to_remove));
        set_truncated_message(event.as_mut_log(), &message, body_len);
        encoded_size = rewrite_log(buf, existing_len, event, include_comma)?;
    }

    if encoded_size > truncation.max_log_bytes {
        buf.truncate(existing_len);
        return Ok(LogEncoding::Dropped {
            reason: "Event remains too large after truncation.",
        });
    }
    emit!(DatadogLogsEventTruncated {
        max_log_bytes: truncation.max_log_bytes,
        max_message_bytes: truncation.max_message_bytes,
        original_encoded_size,
    });
    Ok(LogEncoding::Truncated)
}

fn write_log(buf: &mut Vec<u8>, event: &Event, include_comma: bool) -> Result<usize, io::Error> {
    if include_comma {
        buf.push(b',');
    }
    let object_start = buf.len();
    serde_json::to_writer(&mut *buf, event.as_log())?;
    Ok(buf.len() - object_start)
}

fn rewrite_log(
    buf: &mut Vec<u8>,
    existing_len: usize,
    event: &Event,
    include_comma: bool,
) -> Result<usize, io::Error> {
    buf.truncate(existing_len);
    write_log(buf, event, include_comma)
}

const TRUNCATION_MARKER: &str = "...TRUNCATED...";
const TRUNCATED_TAG: &str = "truncated:single_line";

fn set_truncated_message(log: &mut LogEvent, message: &Bytes, body_len: usize) {
    let marker = TRUNCATION_MARKER.as_bytes();
    let mut truncated = Vec::with_capacity(body_len + marker.len());
    truncated.extend_from_slice(&message[..body_len]);
    truncated.extend_from_slice(marker);
    *message_bytes_mut(log).expect("the message was previously found") = Bytes::from(truncated);
}

fn ensure_truncated_tag(log: &mut LogEvent) {
    let tags_path = event_path!(DDTAGS);
    if let Some(tags) = log.get(tags_path).and_then(Value::as_bytes)
        && !tags.is_empty()
    {
        let tags = String::from_utf8_lossy(tags);
        if tags.split(',').any(|tag| tag.trim() == TRUNCATED_TAG) {
            return;
        }
        log.insert(tags_path, format!("{tags},{TRUNCATED_TAG}"));
    } else {
        log.insert(tags_path, TRUNCATED_TAG);
    }
}

fn message_bytes_mut(log: &mut LogEvent) -> Option<&mut Bytes> {
    match log.as_map_mut()?.get_mut(MESSAGE)? {
        Value::Bytes(message) => Some(message),
        Value::Object(fields) => match fields.get_mut(MESSAGE)? {
            Value::Bytes(message) => Some(message),
            _ => None,
        },
        _ => None,
    }
}

fn strip_non_standard_fields(log: &mut LogEvent, conforms_as_agent: bool) {
    let Some(fields) = log.as_map_mut() else {
        return;
    };
    fields.retain(|field, _| is_reserved_attribute(field.as_str()) || field.as_str() == MESSAGE);
    if conforms_as_agent && let Some(Value::Object(nested)) = fields.get_mut(MESSAGE) {
        nested.retain(|field, _| field.as_str() == MESSAGE);
    }
}

fn floor_char_boundary(bytes: &[u8], mut index: usize) -> usize {
    if index >= bytes.len() {
        return bytes.len();
    }
    while index > 0 && (bytes[index] & 0b1100_0000) == 0b1000_0000 {
        index -= 1;
    }
    index
}

impl<S> LogSink<S>
where
    S: Service<LogApiRequest> + Send + 'static,
    S::Future: Send + 'static,
    S::Response: DriverResponse + Send + 'static,
    S::Error: Debug + Into<crate::Error> + Send,
{
    async fn run_inner(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        let default_api_key = Arc::clone(&self.default_api_key);

        let partitioner = EventPartitioner;
        let batch_settings = self.batch_settings;
        let builder = Arc::new(LogRequestBuilder {
            default_api_key,
            transformer: self.transformer,
            compression: self.compression,
            conforms_as_agent: self.conforms_as_agent,
            max_payload_bytes: self.max_payload_bytes,
            truncation: self.truncation,
        });

        let input = input.batched_partitioned(partitioner, batch_settings.timeout, |_| {
            batch_settings.as_item_size_config(HttpJsonBatchSizer)
        });
        input
            .concurrent_map(default_request_builder_concurrency_limit(), move |input| {
                let builder = Arc::clone(&builder);

                // `concurrent_map` spawns this future on a detached task. The closure itself runs
                // within `run_inner`'s span, so `in_current_span` captures the sink span here and
                // re-enters it on the spawned task to preserve the sink's automatic component tags.
                Box::pin(
                    async move {
                        let (api_key, events) = input;
                        let api_key =
                            api_key.unwrap_or_else(|| Arc::clone(&builder.default_api_key));

                        builder.build_request(events, api_key)
                    }
                    .in_current_span(),
                )
            })
            .filter_map(|request| async move {
                match request {
                    Err(error) => {
                        emit!(SinkRequestBuildError { error });
                        None
                    }
                    Ok(reqs) => Some(futures::stream::iter(reqs)),
                }
            })
            .flatten()
            .into_driver(self.service)
            .protocol(self.protocol)
            .run()
            .await
    }
}

#[async_trait]
impl<S> StreamSink<Event> for LogSink<S>
where
    S: Service<LogApiRequest> + Send + 'static,
    S::Future: Send + 'static,
    S::Response: DriverResponse + Send + 'static,
    S::Error: Debug + Into<crate::Error> + Send,
{
    async fn run(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        self.run_inner(input).await
    }
}

#[cfg(test)]
mod tests {

    use std::sync::Arc;

    use chrono::Utc;
    use vector_lib::{
        config::{LegacyKey, LogNamespace},
        event::{Event, EventMetadata, LogEvent},
        schema::{Definition, meaning},
    };
    use vrl::{
        core::Value,
        event_path, metadata_path, owned_value_path, value,
        value::{Kind, kind::Collection},
    };

    use super::{LogRequestBuilder, normalize_as_agent_event, normalize_event};
    use crate::{
        common::datadog::DD_RESERVED_SEMANTIC_ATTRS,
        sinks::{
            datadog::logs::config::{
                DEFAULT_MAX_LOG_BYTES as MAX_LOG_BYTES, DatadogLogsTruncationConfig,
            },
            util::Compression,
        },
    };

    const TRUNCATION_MARKER: &str = "...TRUNCATED...";
    const TRUNCATED_TAG: &str = "truncated:single_line";

    fn encode_logs(
        events: Vec<Event>,
        max_message_bytes: Option<usize>,
        conforms_as_agent: bool,
    ) -> Vec<serde_json::Value> {
        encode_logs_with_limits(
            events,
            max_message_bytes.map(|max_message_bytes| DatadogLogsTruncationConfig {
                max_log_bytes: MAX_LOG_BYTES,
                max_message_bytes,
            }),
            conforms_as_agent,
        )
    }

    fn encode_logs_with_limits(
        events: Vec<Event>,
        truncation: Option<DatadogLogsTruncationConfig>,
        conforms_as_agent: bool,
    ) -> Vec<serde_json::Value> {
        let requests = LogRequestBuilder {
            default_api_key: Arc::from("unused"),
            transformer: Default::default(),
            compression: Compression::None,
            conforms_as_agent,
            max_payload_bytes: 5_000_000,
            truncation,
        }
        .build_request(events, Arc::from("api-key"))
        .expect("request should build");

        requests
            .into_iter()
            .flat_map(|request| {
                serde_json::from_slice::<Vec<serde_json::Value>>(&request.body)
                    .expect("payload should be a JSON array")
            })
            .collect()
    }

    #[test]
    fn truncates_at_configured_log_limit() {
        let log = LogEvent::from("a".repeat(600));

        let logs = encode_logs_with_limits(
            vec![Event::Log(log)],
            Some(DatadogLogsTruncationConfig {
                max_log_bytes: 500,
                max_message_bytes: 400,
            }),
            false,
        );

        let message = logs[0]["message"]
            .as_str()
            .expect("message should be a string");
        assert_eq!(message.len(), 400 + TRUNCATION_MARKER.len());
        assert!(serde_json::to_vec(&logs[0]).unwrap().len() <= 500);
    }

    #[test]
    fn truncates_oversized_message_and_preserves_other_fields() {
        let original = "a".repeat(MAX_LOG_BYTES + 1);
        let mut log = LogEvent::from(original);
        log.insert(event_path!("service"), "payments");
        log.insert(event_path!("custom"), "keep-me");

        let logs = encode_logs(vec![Event::Log(log)], Some(900_000), false);

        assert_eq!(logs.len(), 1);
        assert_eq!(
            logs[0]["message"]
                .as_str()
                .expect("message should be a string"),
            format!("{}{TRUNCATION_MARKER}", "a".repeat(900_000))
        );
        assert_eq!(logs[0]["service"], "payments");
        assert_eq!(logs[0]["custom"], "keep-me");
        assert_eq!(logs[0]["ddtags"], TRUNCATED_TAG);
        assert!(serde_json::to_vec(&logs[0]).unwrap().len() <= MAX_LOG_BYTES);
    }

    #[test]
    fn truncates_agent_normalized_message_and_preserves_nested_fields() {
        let original = "b".repeat(MAX_LOG_BYTES + 1);
        let mut log = LogEvent::from(original);
        log.insert(event_path!("service"), "payments");
        log.insert(event_path!("custom"), "keep-me");

        let logs = encode_logs(vec![Event::Log(log)], Some(900_000), true);

        let nested = logs[0]["message"]
            .as_object()
            .expect("agent message should be an object");
        let message = nested["message"]
            .as_str()
            .expect("nested message should be a string");
        assert_eq!(message.len(), 900_000 + TRUNCATION_MARKER.len());
        assert!(message.ends_with(TRUNCATION_MARKER));
        assert_eq!(nested["custom"], "keep-me");
        assert_eq!(logs[0]["service"], "payments");
        assert_eq!(logs[0]["ddtags"], TRUNCATED_TAG);
        assert!(serde_json::to_vec(&logs[0]).unwrap().len() <= MAX_LOG_BYTES);
    }

    #[test]
    fn removes_non_standard_fields_if_truncated_log_is_still_oversized() {
        let mut log = LogEvent::from("c".repeat(MAX_LOG_BYTES + 1));
        log.insert(event_path!("service"), "payments");
        log.insert(event_path!("custom"), "x".repeat(200_000));

        let logs = encode_logs(vec![Event::Log(log)], Some(900_000), false);

        assert_eq!(logs.len(), 1);
        assert!(logs[0].get("custom").is_none());
        assert_eq!(logs[0]["service"], "payments");
        assert_eq!(logs[0]["ddtags"], TRUNCATED_TAG);
        assert!(serde_json::to_vec(&logs[0]).unwrap().len() <= MAX_LOG_BYTES);
    }

    #[test]
    fn leaves_short_message_unmarked_when_only_custom_fields_are_removed() {
        let mut log = LogEvent::from("hello");
        log.insert(event_path!("service"), "payments");
        log.insert(event_path!("custom"), "x".repeat(MAX_LOG_BYTES + 1));

        let logs = encode_logs(vec![Event::Log(log)], Some(900_000), false);

        assert_eq!(logs.len(), 1);
        assert_eq!(logs[0]["message"], "hello");
        assert!(logs[0].get("custom").is_none());
        assert_eq!(logs[0]["ddtags"], TRUNCATED_TAG);
    }

    #[test]
    fn removes_nested_non_standard_fields_from_agent_normalized_log() {
        let mut log = LogEvent::from("d".repeat(MAX_LOG_BYTES + 1));
        log.insert(event_path!("service"), "payments");
        log.insert(event_path!("custom"), "x".repeat(200_000));

        let logs = encode_logs(vec![Event::Log(log)], Some(900_000), true);

        assert_eq!(logs.len(), 1);
        let nested = logs[0]["message"]
            .as_object()
            .expect("agent message should be an object");
        assert!(nested.get("custom").is_none());
        assert!(
            nested["message"]
                .as_str()
                .expect("message should be a string")
                .ends_with(TRUNCATION_MARKER)
        );
        assert_eq!(logs[0]["service"], "payments");
        assert_eq!(logs[0]["ddtags"], TRUNCATED_TAG);
        assert!(serde_json::to_vec(&logs[0]).unwrap().len() <= MAX_LOG_BYTES);
    }

    #[test]
    fn drops_log_that_remains_oversized_after_reduction() {
        let mut log = LogEvent::from("e".repeat(MAX_LOG_BYTES + 1));
        log.insert(event_path!("service"), "x".repeat(MAX_LOG_BYTES + 1));

        let logs = encode_logs(vec![Event::Log(log)], Some(900_000), false);

        assert!(logs.is_empty());
    }

    #[test]
    fn counts_irreducible_log_drop_once() {
        vector_lib::metrics::init_test();
        let controller = vector_lib::metrics::Controller::get().unwrap();
        controller.reset();

        let mut log = LogEvent::from("e".repeat(MAX_LOG_BYTES + 1));
        log.insert(event_path!("service"), "x".repeat(MAX_LOG_BYTES + 1));

        let logs = encode_logs(vec![Event::Log(log)], Some(900_000), false);

        assert!(logs.is_empty());
        let discarded_events = controller
            .capture_metrics()
            .iter()
            .filter(|metric| metric.name() == "component_discarded_events_total")
            .map(|metric| match metric.value() {
                vector_lib::event::MetricValue::Counter { value } => *value,
                _ => panic!("discarded events metric must be a counter"),
            })
            .sum::<f64>();
        assert_eq!(discarded_events, 1.0);
    }

    #[test]
    fn drops_oversized_log_with_non_string_message() {
        let mut log = LogEvent::default();
        log.insert(
            event_path!("message"),
            value!({ "body": ("f".repeat(MAX_LOG_BYTES + 1)) }),
        );

        let logs = encode_logs(vec![Event::Log(log)], Some(900_000), false);

        assert!(logs.is_empty());
    }

    #[test]
    fn drops_oversized_log_without_string_message_before_removing_fields() {
        for conforms_as_agent in [false, true] {
            let mut log = LogEvent::default();
            log.insert(event_path!("custom"), "f".repeat(MAX_LOG_BYTES + 1));

            let logs = encode_logs(vec![Event::Log(log)], Some(900_000), conforms_as_agent);

            assert!(logs.is_empty());
        }
    }

    #[test]
    fn shrinks_escaped_message_again_to_fit_default_limits() {
        for conforms_as_agent in [false, true] {
            let log = LogEvent::from("\"".repeat(600_000));

            let logs = encode_logs(vec![Event::Log(log)], Some(900_000), conforms_as_agent);

            assert_eq!(logs.len(), 1);
            let message = if conforms_as_agent {
                &logs[0]["message"]["message"]
            } else {
                &logs[0]["message"]
            }
            .as_str()
            .expect("message should be a string");
            assert!(message.len() < 600_000 + TRUNCATION_MARKER.len());
            assert!(message.ends_with(TRUNCATION_MARKER));
            assert!(serde_json::to_vec(&logs[0]).unwrap().len() <= MAX_LOG_BYTES);
        }
    }

    #[test]
    fn second_message_shrink_reserves_space_for_new_marker() {
        let message = format!("{}{}", "\"".repeat(450_000), "a".repeat(110_000));
        let log = LogEvent::from(message);

        let logs = encode_logs(vec![Event::Log(log)], Some(900_000), false);

        assert_eq!(logs.len(), 1);
        assert!(
            logs[0]["message"]
                .as_str()
                .expect("message should be a string")
                .ends_with(TRUNCATION_MARKER)
        );
        assert!(serde_json::to_vec(&logs[0]).unwrap().len() <= MAX_LOG_BYTES);
    }

    #[test]
    fn does_not_duplicate_truncation_tag() {
        let mut log = LogEvent::from("g".repeat(MAX_LOG_BYTES + 1));
        log.insert(event_path!("ddtags"), format!("env:test,{TRUNCATED_TAG}"));

        let logs = encode_logs(vec![Event::Log(log)], Some(900_000), false);

        assert_eq!(logs[0]["ddtags"], format!("env:test,{TRUNCATED_TAG}"));
    }

    #[test]
    fn appends_truncation_tag_to_existing_tags() {
        let mut log = LogEvent::from("g".repeat(MAX_LOG_BYTES + 1));
        log.insert(event_path!("ddtags"), "env:test");

        let logs = encode_logs(vec![Event::Log(log)], Some(900_000), false);

        assert_eq!(logs[0]["ddtags"], format!("env:test,{TRUNCATED_TAG}"));
    }

    #[test]
    fn leaves_oversized_message_unchanged_when_truncation_is_disabled() {
        let original = "h".repeat(MAX_LOG_BYTES + 1);
        let log = LogEvent::from(original.clone());

        let logs = encode_logs(vec![Event::Log(log)], None, false);

        assert_eq!(logs.len(), 1);
        assert_eq!(logs[0]["message"].as_str(), Some(original.as_str()));
        assert!(logs[0].get("ddtags").is_none());
    }

    #[test]
    fn leaves_log_at_exact_encoded_limit_unchanged() {
        // `{"message":""}` contributes 14 bytes around the message body.
        let original = "h".repeat(MAX_LOG_BYTES - 14);
        let mut log = LogEvent::default();
        log.insert(event_path!("message"), original.clone());

        let logs = encode_logs(vec![Event::Log(log)], Some(900_000), false);

        assert_eq!(serde_json::to_vec(&logs[0]).unwrap().len(), MAX_LOG_BYTES);
        assert_eq!(logs[0]["message"].as_str(), Some(original.as_str()));
        assert!(logs[0].get("ddtags").is_none());
    }

    #[test]
    fn truncation_preserves_utf8_boundaries() {
        let original = "😀".repeat(300_000);
        let log = LogEvent::from(original.clone());

        let logs = encode_logs(vec![Event::Log(log)], Some(900_001), false);

        let message = logs[0]["message"]
            .as_str()
            .expect("message should be valid UTF-8");
        let body = message
            .strip_suffix(TRUNCATION_MARKER)
            .expect("message should have truncation marker");
        assert!(original.starts_with(body));
        assert_eq!(body.len(), 900_000);
    }

    #[test]
    fn delivers_following_log_after_dropping_irreducible_log() {
        let mut oversized = LogEvent::from("i".repeat(MAX_LOG_BYTES + 1));
        oversized.insert(event_path!("service"), "x".repeat(MAX_LOG_BYTES + 1));
        let small = LogEvent::from("ok");

        let logs = encode_logs(
            vec![Event::Log(oversized), Event::Log(small)],
            Some(900_000),
            false,
        );

        assert_eq!(logs.len(), 1);
        assert_eq!(logs[0]["message"], "ok");
    }

    fn assert_normalized_log_has_expected_attrs(log: &LogEvent) {
        assert!(
            log.get(event_path!("timestamp"))
                .expect("should have timestamp")
                .is_integer()
        );

        for attr in [
            "message",
            "timestamp",
            "hostname",
            "ddtags",
            "service",
            "status",
        ] {
            assert!(log.contains(event_path!(attr)), "missing {attr}");
        }

        assert_eq!(
            log.get(event_path!("ddtags")).expect("should have tags"),
            &Value::Bytes("key1:value1,key2:value2".into())
        );
    }

    fn agent_event_metadata(definition: Definition) -> EventMetadata {
        EventMetadata::default().with_schema_definition(&Arc::new(
            definition
                .with_source_metadata(
                    "datadog_agent",
                    Some(LegacyKey::InsertIfEmpty(owned_value_path!("ddtags"))),
                    &owned_value_path!("ddtags"),
                    Kind::bytes(),
                    Some(meaning::TAGS),
                )
                .with_source_metadata(
                    "datadog_agent",
                    Some(LegacyKey::InsertIfEmpty(owned_value_path!("hostname"))),
                    &owned_value_path!("hostname"),
                    Kind::bytes(),
                    Some(meaning::HOST),
                )
                .with_source_metadata(
                    "datadog_agent",
                    Some(LegacyKey::InsertIfEmpty(owned_value_path!("timestamp"))),
                    &owned_value_path!("timestamp"),
                    Kind::timestamp(),
                    Some(meaning::TIMESTAMP),
                )
                .with_source_metadata(
                    "datadog_agent",
                    Some(LegacyKey::InsertIfEmpty(owned_value_path!("severity"))),
                    &owned_value_path!("severity"),
                    Kind::bytes(),
                    Some(meaning::SEVERITY),
                )
                .with_source_metadata(
                    "datadog_agent",
                    Some(LegacyKey::InsertIfEmpty(owned_value_path!("service"))),
                    &owned_value_path!("service"),
                    Kind::bytes(),
                    Some(meaning::SERVICE),
                )
                .with_source_metadata(
                    "datadog_agent",
                    Some(LegacyKey::InsertIfEmpty(owned_value_path!("source"))),
                    &owned_value_path!("source"),
                    Kind::bytes(),
                    Some(meaning::SOURCE),
                ),
        ))
    }

    #[test]
    fn normalize_event_doesnt_require() {
        let mut log = LogEvent::default();
        log.insert(event_path!("foo"), "bar");

        let mut event = Event::Log(log);
        normalize_event(&mut event);

        let log = event.as_log();

        assert!(!log.contains(event_path!("message")));
        assert!(!log.contains(event_path!("timestamp")));
        assert!(!log.contains(event_path!("hostname")));
    }

    #[test]
    fn normalize_event_normalizes_legacy_namespace() {
        let definition = Definition::new_with_default_metadata(
            Kind::object(Collection::empty()),
            [LogNamespace::Legacy],
        );
        let mut log = LogEvent::new_with_metadata(agent_event_metadata(definition));
        log.insert(event_path!("message"), "the_message");
        let namespace = log.namespace();

        namespace.insert_standard_vector_source_metadata(&mut log, "datadog_agent", Utc::now());

        let tags = vec![
            Value::Bytes("key1:value1".into()),
            Value::Bytes("key2:value2".into()),
        ];

        log.insert(event_path!("ddtags"), tags);
        log.insert(event_path!("hostname"), "the_host");
        log.insert(event_path!("service"), "the_service");
        log.insert(event_path!("source"), "the_source");
        log.insert(event_path!("severity"), "the_severity");

        assert!(log.namespace() == LogNamespace::Legacy);

        let mut event = Event::Log(log);
        normalize_event(&mut event);

        assert_normalized_log_has_expected_attrs(event.as_log());
    }

    #[test]
    fn normalize_event_normalizes_vector_namespace_raw_field() {
        let mut event = prepare_event_vector_namespace(|definition| {
            LogEvent::from_parts(value!("the_message"), agent_event_metadata(definition))
        });

        normalize_event(&mut event);
        normalize_as_agent_event(&mut event);

        assert_normalized_log_has_expected_attrs(event.as_log());
        assert_only_reserved_fields_at_root(event.as_log());
        assert_eq!(
            event.as_log().get(event_path!("message")),
            Some(&value!({"message": "the_message"}))
        );
    }

    fn prepare_event_vector_namespace(log_generator: fn(Definition) -> LogEvent) -> Event {
        let definition =
            Definition::new_with_default_metadata(Kind::bytes(), [LogNamespace::Vector]);
        let mut log = log_generator(definition);

        // insert an arbitrary metadata field such that the log becomes Vector namespaced
        log.insert(metadata_path!("vector", "foo"), "bar");

        let namespace = log.namespace();
        namespace.insert_standard_vector_source_metadata(&mut log, "datadog_agent", Utc::now());

        let tags = vec![
            Value::Bytes("key1:value1".into()),
            Value::Bytes("key2:value2".into()),
        ];
        log.insert(metadata_path!("datadog_agent", "ddtags"), tags);

        log.insert(metadata_path!("datadog_agent", "hostname"), "the_host");
        log.insert(metadata_path!("datadog_agent", "timestamp"), Utc::now());
        log.insert(metadata_path!("datadog_agent", "service"), "the_service");
        log.insert(metadata_path!("datadog_agent", "source"), "the_source");
        log.insert(metadata_path!("datadog_agent", "severity"), "the_severity");

        assert!(log.namespace() == LogNamespace::Vector);
        Event::Log(log)
    }

    #[test]
    fn normalize_event_normalizes_vector_namespace() {
        let mut event = prepare_event_vector_namespace(|definition| {
            let mut log = LogEvent::new_with_metadata(agent_event_metadata(definition));
            log.insert(event_path!("message"), "the_message");
            log
        });

        normalize_event(&mut event);
        normalize_as_agent_event(&mut event);

        assert_normalized_log_has_expected_attrs(event.as_log());
        assert_only_reserved_fields_at_root(event.as_log());
    }

    fn prepare_agent_event() -> LogEvent {
        let definition = Definition::new_with_default_metadata(
            Kind::object(Collection::empty()),
            [LogNamespace::Legacy],
        );
        let mut log = LogEvent::new_with_metadata(agent_event_metadata(definition));
        let namespace = log.namespace();
        namespace.insert_standard_vector_source_metadata(&mut log, "datadog_agent", Utc::now());

        let tags = vec![
            Value::Bytes("key1:value1".into()),
            Value::Bytes("key2:value2".into()),
        ];

        // insert mandatory fields
        log.insert(event_path!("ddtags"), tags);
        log.insert(event_path!("hostname"), "the_host");
        log.insert(event_path!("service"), "the_service");
        log.insert(event_path!("timestamp"), Utc::now());
        log.insert(event_path!("source"), "the_source");
        log.insert(event_path!("severity"), "the_severity");

        let sample_message = value!({
            "message": "hello world",
            "field_a": "field_a_value",
            "field_b": "field_b_value",
            "field_c": { "field_c_nested" : "field_c_value" },
        });
        log.insert(event_path!("message"), sample_message.to_string());
        log
    }

    fn assert_only_reserved_fields_at_root(log: &LogEvent) {
        let objmap = log.as_map().unwrap();
        let reserved_fields = DD_RESERVED_SEMANTIC_ATTRS
            .into_iter()
            .chain([("message", "message")])
            .collect::<Vec<(&str, &str)>>();
        for key in objmap.keys() {
            assert!(reserved_fields.iter().any(|(_, msg)| *msg == key.as_str()));
        }
    }

    #[test]
    fn normalize_conforming_agent_with_collisions() {
        let mut log = prepare_agent_event();

        // insert random fields at root which will collide with sample data at 'message'
        log.insert(event_path!("field_a"), "replaced_field_a_value");
        log.insert(event_path!("field_c"), "replaced_field_c_value");
        let mut event = Event::Log(log);
        normalize_event(&mut event);
        normalize_as_agent_event(&mut event);

        let log = event.as_log();
        assert_normalized_log_has_expected_attrs(log);
        assert_only_reserved_fields_at_root(log);
        assert_eq!(
            log.get(event_path!("message")),
            Some(&value!({
                "source_type": "datadog_agent",
                "field_a": "replaced_field_a_value",
                "field_c": "replaced_field_c_value",
                "message": (value!({
                    "message": "hello world",
                    "field_a": "field_a_value",
                    "field_b": "field_b_value",
                    "field_c": { "field_c_nested" : "field_c_value" },
                }).to_string()),
            }))
        );
    }

    #[test]
    fn normalize_conforming_agent() {
        let mut log = prepare_agent_event();

        // insert random fields at root
        log.insert(event_path!("field_1"), "value_1");
        log.insert(event_path!("field_2"), "value_2");
        log.insert(event_path!("field_3", "field_3_nested"), "value_3");

        // normalize and validate...
        let mut event = Event::Log(log);
        normalize_event(&mut event);
        normalize_as_agent_event(&mut event);

        // that all fields placed at the root no longer exist there
        let log = event.as_log();
        assert_normalized_log_has_expected_attrs(log);
        assert_only_reserved_fields_at_root(log);

        // .. and that they were nested properly underneath message
        assert_eq!(
            log.get(event_path!("message")),
            Some(&value!({
                "source_type": "datadog_agent",
                "message": (value!({
                    "message": "hello world",
                    "field_a": "field_a_value",
                    "field_b": "field_b_value",
                    "field_c": { "field_c_nested" : "field_c_value" },
                }).to_string()),
                "field_1": "value_1",
                "field_2": "value_2",
                "field_3": {
                    "field_3_nested": "value_3"
                }
            }))
        );
    }
}
