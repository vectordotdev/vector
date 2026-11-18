//! Streams aggregation adapter, retaining original-event ownership through delivery.

use std::io::Write;
#[cfg(any(test, feature = "sinks-aws_kinesis_streams"))]
use std::time::Duration;

use bytes::Bytes;
use vector_lib::{
    configurable::configurable_component,
    internal_event::{ComponentEventsDropped, UNINTENTIONAL, emit},
    request_metadata::RequestMetadata,
};

use super::{
    aggregation::{Aggregate, Aggregator, Format, Settings},
    record::Record,
    request_builder::{KinesisRequest, KinesisRequestBuilder},
    sink::{KinesisKey, KinesisProcessedEvent, gen_partition_key},
};
#[cfg(any(test, feature = "sinks-aws_kinesis_streams"))]
use crate::codecs::encoding::SerializerConfig;
use crate::{
    event::{Event, EventFinalizers, EventStatus, Finalizable},
    internal_events::{AwsKinesisStreamNoPartitionKeyError, SinkRequestBuildError},
    sinks::{
        prelude::*,
        util::{Compressor, RequestBuilder, encoding::Encoder as _, request_builder::EncodeResult},
    },
};

const RECORD_LIMIT: usize = 1_048_576;

/// Combine serialized logs into newline-delimited or KPL Kinesis records.
#[configurable_component]
#[derive(Clone, Debug)]
#[serde(default, deny_unknown_fields)]
pub struct AggregationConfig {
    /// Enable aggregation. Consumers must support the selected aggregation format.
    pub enabled: bool,
    /// Aggregation format. Newline preserves text/JSONL; KPL uses the standard protobuf envelope.
    pub format: Format,
    /// Maximum original log events in each Kinesis record.
    /// Must be positive and at most 1000 for KPL. This is separate from the API request's `batch.max_events`.
    pub max_events: usize,
    /// Target data bytes per record, including delimiters or the KPL envelope.
    /// Newline counts uncompressed bytes; KPL counts independently compressed user payloads.
    /// Must be in 1..=1048576. Actual key bytes and compression can force smaller records.
    pub max_bytes: usize,
    /// Maximum time to retain the first event in an aggregate, in seconds.
    /// Must be finite and positive. Request batching can add further buffering time.
    pub timeout_secs: f64,
}

impl Default for AggregationConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            format: Format::Newline,
            max_events: 500,
            max_bytes: 943_718,
            timeout_secs: 1.0,
        }
    }
}

#[cfg(any(test, feature = "sinks-aws_kinesis_streams"))]
impl AggregationConfig {
    pub(crate) fn settings(
        &self,
        serializer: &SerializerConfig,
    ) -> crate::Result<Option<Settings>> {
        if !self.enabled {
            return Ok(None);
        }
        if self.max_events == 0 || self.max_bytes == 0 || self.max_bytes > RECORD_LIMIT {
            return Err("aggregation requires max_events > 0 and max_bytes in 1..=1048576".into());
        }
        let timeout = Duration::try_from_secs_f64(self.timeout_secs)
            .ok()
            .filter(|duration| {
                !duration.is_zero() && std::time::Instant::now().checked_add(*duration).is_some()
            })
            .ok_or("aggregation.timeout_secs must be finite, positive, and representable")?;
        if self.format == Format::Newline {
            match serializer {
                SerializerConfig::Text(_) => {}
                SerializerConfig::Json(config) if !config.options.pretty => {}
                _ => {
                    return Err("newline aggregation requires text or compact JSON encoding".into());
                }
            }
        }
        #[cfg(feature = "sinks-aws_kinesis_streams")]
        if self.format == Format::Kpl && self.max_events > 1000 {
            return Err("KPL aggregation requires max_events in 1..=1000".into());
        }
        Ok(Some(Settings {
            format: self.format,
            max_events: self.max_events,
            max_bytes: self.max_bytes,
            timeout,
        }))
    }
}

pub(crate) struct EventMetadata {
    finalizers: EventFinalizers,
    metadata: RequestMetadata,
}

fn reject(items: Vec<EventMetadata>, error: std::io::Error) {
    let count = items.iter().map(|item| item.metadata.event_count()).sum();
    for item in items {
        item.finalizers.update_status(EventStatus::Rejected);
    }
    emit(SinkRequestBuildError { error });
    emit(ComponentEventsDropped::<UNINTENTIONAL> {
        count,
        reason: "Kinesis aggregation record build failed.",
    });
}

/// Build records after compression, splitting only at original-event boundaries.
fn build_records<R: Record>(
    batch: Aggregate<EventMetadata>,
    builder: &KinesisRequestBuilder<R>,
    format: Format,
) -> Vec<KinesisRequest<R>> {
    let mut pending = vec![batch];
    let mut output = Vec::new();
    while let Some(batch) = pending.pop() {
        let key = batch
            .partition_key
            .clone()
            .unwrap_or_else(gen_partition_key);
        let payload = match format {
            Format::Newline => compress(&batch.payload, builder.compression),
            #[cfg(feature = "sinks-aws_kinesis_streams")]
            Format::Kpl => Ok(Bytes::from(super::kpl::envelope(&key, &batch.payload))),
        };
        let payload = match payload {
            Ok(payload) => payload,
            Err(error) => {
                reject(batch.items, error);
                continue;
            }
        };
        if payload.len().saturating_add(key.len()) > RECORD_LIMIT
            || batch.payload.len().saturating_add(key.len()) > RECORD_LIMIT
        {
            if batch.items.len() > 1 {
                let (left, right) = batch.split();
                pending.push(right);
                pending.push(left);
            } else {
                reject(
                    batch.items,
                    std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        "Kinesis record exceeds 1 MiB including its partition key",
                    ),
                );
            }
            continue;
        }
        let metadata =
            RequestMetadata::from_batch(batch.items.iter().map(|item| item.metadata.clone()));
        let metadata = RequestMetadata::new(
            metadata.event_count(),
            metadata.events_byte_size(),
            if format == Format::Newline {
                batch.payload.len()
            } else {
                metadata.request_encoded_size()
            },
            payload.len(),
            metadata.into_events_estimated_json_encoded_byte_size(),
        );
        output.push(KinesisRequest {
            key: KinesisKey {
                partition_key: key.clone(),
            },
            record: R::new(&payload, &key),
            finalizers: batch
                .items
                .into_iter()
                .map(|item| item.finalizers)
                .collect(),
            metadata,
        });
    }
    output
}

fn compress(payload: &[u8], compression: Compression) -> std::io::Result<Bytes> {
    let mut compressor = Compressor::from(compression);
    compressor.write_all(payload)?;
    compressor.finish().map(bytes::BytesMut::freeze)
}

/// Serialize and aggregate original logs, retaining their metadata and finalizers.
#[doc(hidden)]
pub fn requests<R>(
    mut input: BoxStream<'_, Event>,
    builder: KinesisRequestBuilder<R>,
    key_field: Option<vector_lib::lookup::lookup_v2::ConfigValuePath>,
    settings: Settings,
) -> BoxStream<'_, KinesisRequest<R>>
where
    R: Record + Send + Sync + 'static,
    R::T: Send,
{
    Box::pin(async_stream::stream! {
        let mut aggregator = Aggregator::new(settings);
        loop {
            let event = if let Some(deadline) = aggregator.deadline() {
                tokio::select! {
                    biased;
                    () = tokio::time::sleep_until(tokio::time::Instant::from_std(deadline)) => {
                        if let Some(batch) = aggregator.flush() {
                            for request in build_records(batch, &builder, settings.format) {
                                yield request;
                                tokio::task::yield_now().await;
                            }
                        }
                        continue;
                    }
                    event = input.next() => event,
                }
            } else {
                input.next().await
            };
            let Some(event) = event else {
                break;
            };
            let mut log = event.into_log();
            let key = if let Some(field) = &key_field {
                let Some(value) = log.get((vrl::path::PathPrefix::Event, field)) else {
                    log.take_finalizers().update_status(EventStatus::Rejected);
                    emit(AwsKinesisStreamNoPartitionKeyError {
                        partition_key_field: field.0.to_string().as_str()
                    });
                    continue;
                };
                let mut key = value.to_string_lossy().into_owned();
                key.truncate(key.floor_char_boundary(256));
                Some(key)
            } else {
                None
            };
            let processed = KinesisProcessedEvent {
                event: log,
                metadata: KinesisKey { partition_key: String::new() },
            };
            let (metadata, stats, event) = builder.split_input(processed);
            let mut bytes = Vec::new();
            let transformed_size = match builder.encoder.encode_input(event, &mut bytes) {
                Ok((_, size)) => size,
                Err(error) => {
                    metadata.finalizers.update_status(EventStatus::Rejected);
                    emit(SinkRequestBuildError { error });
                    continue;
                }
            };
            let encoded = EncodeResult::uncompressed(Bytes::from(bytes), transformed_size);
            let item = EventMetadata {
                finalizers: metadata.finalizers,
                metadata: stats.build(&encoded),
            };
            let payload = match settings.format {
                Format::Newline => Ok(encoded.payload),
                #[cfg(feature = "sinks-aws_kinesis_streams")]
                Format::Kpl => {
                    if builder.compression.is_compressed() {
                        compress(&encoded.payload, builder.compression)
                    } else {
                        Ok(encoded.payload)
                    }
                }
            };
            let payload = match payload {
                Ok(payload) => payload,
                Err(error) => { reject(vec![item], error); continue; }
            };
            let key_len = key.as_ref().map_or(0, String::len);
            let size = settings.format.event_len(&payload) + settings.format.overhead(key_len);
            if size.saturating_add(key_len) > RECORD_LIMIT {
                reject(vec![item], std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "Kinesis log exceeds the record limit",
                ));
                continue;
            }
            for batch in aggregator.push(
                &payload,
                key,
                item,
                tokio::time::Instant::now().into_std(),
            ) {
                for request in build_records(batch, &builder, settings.format) {
                    yield request;
                    // Let the driver's ready_chunks release completed API batches before
                    // serializing more input, so service backpressure reaches this stream.
                    tokio::task::yield_now().await;
                }
            }
        }
        if let Some(batch) = aggregator.flush() {
            for request in build_records(batch, &builder, settings.format) {
                yield request;
                tokio::task::yield_now().await;
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use std::{
        io::Read,
        marker::PhantomData,
        sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        },
    };

    use super::*;
    use crate::{
        codecs::{Encoder, Transformer},
        event::{BatchNotifier, BatchStatus},
        sinks::util::Compression,
    };
    use vector_lib::{
        codecs::{JsonSerializerConfig, TextSerializerConfig},
        request_metadata::MetaDescriptive,
    };

    #[derive(Clone)]
    struct Captured {
        data: Bytes,
        key: String,
    }
    impl Record for Captured {
        type T = Self;
        fn new(data: &Bytes, key: &str) -> Self {
            Self {
                data: data.clone(),
                key: key.into(),
            }
        }
        fn encoded_length(&self) -> usize {
            self.data.len() + self.key.len()
        }
        fn get(self) -> Self {
            self
        }
    }
    fn builder(json: bool, compression: Compression) -> KinesisRequestBuilder<Captured> {
        let serializer = if json {
            JsonSerializerConfig::default().build().into()
        } else {
            TextSerializerConfig::default().build().into()
        };
        KinesisRequestBuilder {
            compression,
            encoder: (Transformer::default(), Encoder::<()>::new(serializer)),
            _phantom: PhantomData,
        }
    }
    fn settings() -> Settings {
        Settings {
            format: Format::Newline,
            max_events: 500,
            max_bytes: 943_718,
            timeout: Duration::from_secs(1),
        }
    }
    fn input(lines: &[&str]) -> BoxStream<'static, Event> {
        futures::stream::iter(lines.iter().map(|line| event(*line)).collect::<Vec<_>>()).boxed()
    }
    fn event(message: impl Into<LogEvent>) -> Event {
        Event::Log(message.into())
    }
    fn item() -> EventMetadata {
        EventMetadata {
            finalizers: EventFinalizers::default(),
            metadata: RequestMetadata::new(1, 0, 0, 0, GroupedCountByteSize::default()),
        }
    }

    #[cfg(feature = "sinks-aws_kinesis_streams")]
    fn kpl_settings() -> Settings {
        toml::from_str::<AggregationConfig>("enabled = true\nformat = 'kpl'")
            .unwrap()
            .settings(&SerializerConfig::Text(TextSerializerConfig::default()))
            .unwrap()
            .unwrap()
    }

    #[cfg(feature = "sinks-aws_kinesis_streams")]
    #[tokio::test]
    async fn kpl_emits_standard_envelope_and_preserves_payload_boundaries() {
        let mut a = event("a");
        a.as_mut_log().insert(vrl::event_path!("key"), "key");
        let mut b = event("b\n");
        b.as_mut_log().insert(vrl::event_path!("key"), "key");
        let output = requests(
            futures::stream::iter([a, b]).boxed(),
            builder(false, Compression::None),
            Some("key".into()),
            kpl_settings(),
        )
        .collect::<Vec<_>>()
        .await;
        assert_eq!(output.len(), 1);
        assert_eq!(&output[0].record.data[..4], &[0xf3, 0x89, 0x9a, 0xc2]);
        assert_eq!(
            &output[0].record.data[4..24],
            &[
                0x0a, 3, b'k', b'e', b'y', 0x1a, 5, 8, 0, 0x1a, 1, b'a', 0x1a, 6, 8, 0, 0x1a, 2,
                b'b', b'\n',
            ]
        );
        assert_eq!(
            &output[0].record.data[24..],
            &[
                0x4d, 0xaa, 0x40, 0xa3, 0xb8, 0xeb, 0x7a, 0xa2, 0xfb, 0xc4, 0xf2, 0x91, 0x79, 0x37,
                0xf6, 0xaf,
            ]
        );
        assert_eq!(output[0].get_metadata().event_count(), 2);
    }

    #[cfg(feature = "sinks-aws_kinesis_streams")]
    #[tokio::test]
    async fn kpl_binary_empty_and_varint_boundaries_round_trip() {
        for size in [0, 1, 124, 125, 127, 128, 16380, 16384] {
            let bytes = Bytes::from(
                (0_u32..size)
                    .map(|index| index.to_le_bytes()[0])
                    .collect::<Vec<_>>(),
            );
            let mut log = LogEvent::from("unused");
            log.insert(vrl::event_path!("message"), bytes.clone());
            let output = requests(
                futures::stream::iter([Event::Log(log)]).boxed(),
                builder(false, Compression::None),
                None,
                kpl_settings(),
            )
            .collect::<Vec<_>>()
            .await;
            assert_eq!(output.len(), 1);
            let record = &output[0].record;
            assert_eq!(
                super::super::kpl::test_support::decode(&record.data, &record.key),
                [bytes.to_vec()]
            );
        }
    }

    #[cfg(feature = "sinks-aws_kinesis_streams")]
    #[tokio::test]
    async fn kpl_preserves_backpressure_with_continuously_ready_input() {
        let consumed = Arc::new(AtomicUsize::new(0));
        let observed = Arc::clone(&consumed);
        let events = futures::stream::repeat_with(move || {
            observed.fetch_add(1, Ordering::Relaxed);
            event("x".repeat(10000))
        });
        let output = requests(
            events.boxed(),
            builder(false, Compression::None),
            None,
            Settings {
                max_events: 2,
                ..kpl_settings()
            },
        );
        let chunk = output.ready_chunks(1024).next().await.unwrap();
        assert!(chunk.len() <= 2);
        assert!(consumed.load(Ordering::Relaxed) <= 4);
        assert_eq!(
            super::super::kpl::test_support::decode(&chunk[0].record.data, &chunk[0].record.key)
                .len(),
            2
        );
    }

    #[cfg(feature = "sinks-aws_kinesis_streams")]
    #[tokio::test]
    async fn kpl_compresses_individual_json_payloads_and_retains_metadata() {
        let output = requests(
            input(&["a", "b"]),
            builder(true, Compression::gzip_default()),
            None,
            kpl_settings(),
        )
        .collect::<Vec<_>>()
        .await;
        assert_eq!(output.len(), 1);
        let request = &output[0];
        let payloads =
            super::super::kpl::test_support::decode(&request.record.data, &request.record.key);
        assert_eq!(payloads.len(), 2);
        for (payload, expected) in payloads.iter().zip(["a", "b"]) {
            let mut bytes = Vec::new();
            vector_common::decompression::CappedDecoder::gzip(payload.as_slice())
                .into_reader()
                .read_to_end(&mut bytes)
                .unwrap();
            assert_eq!(
                serde_json::from_slice::<serde_json::Value>(&bytes).unwrap()["message"],
                expected
            );
            assert!(!bytes.ends_with(b"\n"));
        }
        assert_eq!(request.get_metadata().event_count(), 2);
        assert_eq!(
            request.get_metadata().request_wire_size(),
            request.record.data.len()
        );
    }

    #[cfg(feature = "sinks-aws_kinesis_streams")]
    #[tokio::test]
    async fn kpl_exact_hard_limit_rejects_only_oversized_original() {
        let (batch, status) = BatchNotifier::new_with_receiver();
        let mut fits = event("x".repeat(RECORD_LIMIT - 38));
        fits.as_mut_log().insert(vrl::event_path!("key"), "key");
        let mut oversized = event("x".repeat(RECORD_LIMIT - 37)).with_batch_notifier(&batch);
        oversized
            .as_mut_log()
            .insert(vrl::event_path!("key"), "key");
        drop(batch);
        let output = requests(
            futures::stream::iter([fits, oversized]).boxed(),
            builder(false, Compression::None),
            Some("key".into()),
            kpl_settings(),
        )
        .collect::<Vec<_>>()
        .await;
        assert_eq!(status.await, BatchStatus::Rejected);
        assert_eq!(output.len(), 1);
        assert_eq!(
            output[0].record.data.len() + output[0].record.key.len(),
            RECORD_LIMIT
        );
        assert_eq!(
            super::super::kpl::test_support::decode(&output[0].record.data, "key")[0].len(),
            RECORD_LIMIT - 38
        );
    }

    #[cfg(feature = "sinks-aws_kinesis_streams")]
    #[tokio::test]
    async fn kpl_splits_at_original_boundaries_for_actual_outer_key_limit() {
        let key = "é".repeat(128);
        let events = ["a", "b"].map(|value| {
            let mut event = event(value.repeat(RECORD_LIMIT / 2 - 200));
            event
                .as_mut_log()
                .insert(vrl::event_path!("key"), key.clone());
            event
        });
        let output = requests(
            futures::stream::iter(events).boxed(),
            builder(false, Compression::None),
            Some("key".into()),
            Settings {
                max_bytes: RECORD_LIMIT,
                ..kpl_settings()
            },
        )
        .collect::<Vec<_>>()
        .await;
        assert_eq!(output.len(), 2);
        for (request, value) in output.iter().zip(*b"ab") {
            assert!(request.record.data.len() + request.record.key.len() <= RECORD_LIMIT);
            assert_eq!(
                super::super::kpl::test_support::decode(&request.record.data, &key),
                [vec![value; RECORD_LIMIT / 2 - 200]]
            );
            assert_eq!(request.get_metadata().event_count(), 1);
        }
    }

    #[cfg(feature = "sinks-aws_kinesis_streams")]
    #[tokio::test(start_paused = true)]
    async fn kpl_count_keys_idle_deadline_and_eof_preserve_order() {
        let mut settings = kpl_settings();
        settings.max_events = 2;
        let events = ["a", "b", "c", "d"].map(|value| {
            let mut event = event(value);
            event.as_mut_log().insert(
                vrl::event_path!("key"),
                if value == "a" { "one" } else { "two" },
            );
            event
        });
        let stream = futures::stream::iter(events)
            .chain(futures::stream::pending())
            .boxed();
        let start = tokio::time::Instant::now();
        let mut output = requests(
            stream,
            builder(false, Compression::None),
            Some("key".into()),
            settings,
        );
        for (expected_key, expected_payloads) in [
            ("one", vec![b"a".to_vec()]),
            ("two", vec![b"b".to_vec(), b"c".to_vec()]),
            ("two", vec![b"d".to_vec()]),
        ] {
            let request = output.next().await.unwrap();
            assert_eq!(
                super::super::kpl::test_support::decode(&request.record.data, expected_key),
                expected_payloads
            );
        }
        assert_eq!(start.elapsed(), Duration::from_secs(1));
    }

    #[test]
    fn legacy_request_builder_keeps_single_log_bytes_without_delimiters() {
        let builder = builder(false, Compression::None);
        let processed = KinesisProcessedEvent {
            event: LogEvent::from("raw"),
            metadata: KinesisKey {
                partition_key: "key".into(),
            },
        };
        let (metadata, stats, event) = builder.split_input(processed);
        let encoded = builder.encode_events(event).unwrap();
        let stats = stats.build(&encoded);
        let request = builder.build_request(metadata, stats, encoded);
        assert_eq!(request.record.data, "raw");
        assert_eq!(request.get_metadata().event_count(), 1);
    }

    #[tokio::test]
    async fn empty_input_and_embedded_newlines_preserve_bytes() {
        let empty = requests(
            input(&[]),
            builder(false, Compression::None),
            None,
            settings(),
        )
        .collect::<Vec<_>>()
        .await;
        assert!(empty.is_empty());
        let output = requests(
            input(&["", "a\nb", "c\n"]),
            builder(false, Compression::None),
            None,
            settings(),
        )
        .collect::<Vec<_>>()
        .await;
        assert_eq!(output.len(), 1);
        assert_eq!(output[0].record.data, "\na\nb\nc\n");
        assert_eq!(output[0].get_metadata().event_count(), 3);
    }

    #[tokio::test]
    async fn ready_input_does_not_fill_the_drivers_large_prefetch_chunk() {
        let count = Arc::new(AtomicUsize::new(0));
        let read = Arc::clone(&count);
        let events = futures::stream::repeat_with(move || {
            read.fetch_add(1, Ordering::Relaxed);
            event("x".repeat(10_000))
        });
        let output = requests(
            events.boxed(),
            builder(false, Compression::None),
            None,
            Settings {
                max_events: 2,
                ..settings()
            },
        );
        let chunk = output.ready_chunks(1024).next().await.unwrap();
        assert!(
            chunk.len() <= 2,
            "a stalled service must not prefetch hundreds of large records"
        );
        assert!(count.load(Ordering::Relaxed) <= 4);
    }

    #[test]
    fn validates_limits_codecs_and_extreme_timeouts() {
        let text = SerializerConfig::Text(TextSerializerConfig::default());
        let mut config = AggregationConfig {
            enabled: true,
            ..Default::default()
        };
        assert!(config.settings(&text).unwrap().is_some());
        for timeout in [0.0, -1.0, f64::INFINITY, f64::NAN, f64::MAX] {
            config.timeout_secs = timeout;
            assert!(config.settings(&text).is_err());
        }
        if std::time::Instant::now()
            .checked_add(Duration::from_secs(1_000_000_000_000))
            .is_none()
        {
            config.timeout_secs = 1e12;
            assert!(config.settings(&text).is_err());
        }
        config.timeout_secs = 1.0;
        config.max_bytes = RECORD_LIMIT + 1;
        assert!(config.settings(&text).is_err());
        config.enabled = false;
        assert!(config.settings(&text).unwrap().is_none());
        let json: SerializerConfig = toml::from_str("codec = 'json'\njson.pretty = true").unwrap();
        config.enabled = true;
        config.max_bytes = 100;
        assert!(config.settings(&json).is_err());
    }

    #[tokio::test]
    async fn text_exact_bytes_original_counts_and_independent_finalizers() {
        let (one, mut r1) = BatchNotifier::new_with_receiver();
        let (two, mut r2) = BatchNotifier::new_with_receiver();
        let events = vec![
            event("a").with_batch_notifier(&one),
            event("b\n").with_batch_notifier(&two),
        ];
        drop((one, two));
        let mut output = requests(
            futures::stream::iter(events).boxed(),
            builder(false, Compression::None),
            None,
            settings(),
        )
        .collect::<Vec<_>>()
        .await;
        assert_eq!(output.len(), 1);
        assert_eq!(output[0].record.data, "a\nb\n");
        assert_eq!(output[0].get_metadata().event_count(), 2);
        assert_eq!(output[0].finalizers.len(), 2);
        assert!(r1.try_recv().is_err());
        assert!(r2.try_recv().is_err());
        output[0]
            .take_finalizers()
            .update_status(EventStatus::Delivered);
        assert_eq!(r1.await, BatchStatus::Delivered);
        assert_eq!(r2.await, BatchStatus::Delivered);
    }

    #[tokio::test]
    async fn compact_json_and_whole_record_compression() {
        let output = requests(
            input(&["a", "b"]),
            builder(true, Compression::gzip_default()),
            None,
            settings(),
        )
        .collect::<Vec<_>>()
        .await;
        assert_eq!(output.len(), 1);
        let mut data = String::new();
        vector_common::decompression::CappedDecoder::gzip(output[0].record.data.as_ref())
            .into_reader()
            .read_to_string(&mut data)
            .unwrap();
        let lines = data
            .lines()
            .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0]["message"], "a");
        assert_eq!(lines[1]["message"], "b");
        assert!(data.ends_with('\n'));
    }

    #[tokio::test(start_paused = true)]
    async fn idle_input_flushes_without_eof() {
        let stream = input(&["a"]).chain(futures::stream::pending());
        let start = tokio::time::Instant::now();
        let mut output = requests(
            stream.boxed(),
            builder(false, Compression::None),
            None,
            settings(),
        );
        assert_eq!(output.next().await.unwrap().record.data, "a\n");
        assert_eq!(start.elapsed(), Duration::from_secs(1));
    }

    #[tokio::test(start_paused = true)]
    async fn ready_input_deadline_and_downstream_backpressure() {
        let count = Arc::new(AtomicUsize::new(0));
        let read = Arc::clone(&count);
        let events = futures::stream::unfold((), move |()| {
            let read = Arc::clone(&read);
            async move {
                tokio::time::advance(Duration::from_millis(100)).await;
                read.fetch_add(1, Ordering::Relaxed);
                Some((event("x"), ()))
            }
        });
        let mut output = requests(
            events.boxed(),
            builder(false, Compression::None),
            None,
            settings(),
        );
        let record = output.next().await.unwrap();
        assert!(record.metadata.event_count() <= 11);
        let consumed = count.load(Ordering::Relaxed);
        tokio::time::advance(Duration::from_secs(5)).await;
        assert_eq!(count.load(Ordering::Relaxed), consumed);
        assert!(output.next().await.is_some());
    }

    #[tokio::test]
    async fn keys_are_utf8_safe_and_missing_keys_reject() {
        let key = "é".repeat(200);
        let (batch, status) = BatchNotifier::new_with_receiver();
        let mut a = event("a");
        a.as_mut_log().insert(vrl::event_path!("key"), key);
        let mut b = event("b");
        b.as_mut_log().insert(vrl::event_path!("key"), "other");
        let missing = event("missing").with_batch_notifier(&batch);
        drop(batch);
        let output = requests(
            futures::stream::iter(vec![a, missing, b]).boxed(),
            builder(false, Compression::None),
            Some("key".into()),
            settings(),
        )
        .collect::<Vec<_>>()
        .await;
        assert_eq!(output.len(), 2);
        assert_eq!(output[0].record.key, "é".repeat(128));
        assert_eq!(output[1].record.key, "other");
        assert_eq!(status.await, BatchStatus::Rejected);
    }

    #[tokio::test]
    async fn oversized_rejection_does_not_lose_later_events() {
        let (batch, status) = BatchNotifier::new_with_receiver();
        let events = vec![
            event("x".repeat(RECORD_LIMIT)).with_batch_notifier(&batch),
            event("ok"),
        ];
        drop(batch);
        let output = requests(
            futures::stream::iter(events).boxed(),
            builder(false, Compression::None),
            None,
            settings(),
        )
        .collect::<Vec<_>>()
        .await;
        assert_eq!(output.len(), 1);
        assert_eq!(output[0].record.data, "ok\n");
        assert_eq!(status.await, BatchStatus::Rejected);
    }

    #[test]
    fn checks_actual_key_bytes_and_splits_compression_expansion_in_order() {
        let mut aggregator = Aggregator::new(Settings {
            max_bytes: RECORD_LIMIT,
            ..settings()
        });
        aggregator.push(
            &vec![b'x'; RECORD_LIMIT - 2],
            Some("k".into()),
            item(),
            std::time::Instant::now(),
        );
        let output = build_records(
            aggregator.flush().unwrap(),
            &builder(false, Compression::None),
            Format::Newline,
        );
        assert_eq!(output.len(), 1);
        assert_eq!(
            output[0].record.data.len() + output[0].record.key.len(),
            RECORD_LIMIT
        );
        let mut random = Vec::with_capacity(RECORD_LIMIT - 2);
        let mut state = 42u64;
        for _ in 0..RECORD_LIMIT - 2 {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            random.push(state.to_le_bytes()[0]);
        }
        random[RECORD_LIMIT / 2 - 2] = b'\n';
        random[RECORD_LIMIT - 3] = b'\n';
        let mut aggregator = Aggregator::new(Settings {
            max_bytes: RECORD_LIMIT,
            ..settings()
        });
        let now = std::time::Instant::now();
        aggregator.push(
            &random[..RECORD_LIMIT / 2 - 1],
            Some("k".into()),
            item(),
            now,
        );
        aggregator.push(
            &random[RECORD_LIMIT / 2 - 1..],
            Some("k".into()),
            item(),
            now,
        );
        let output = build_records(
            aggregator.flush().unwrap(),
            &builder(false, Compression::gzip_default()),
            Format::Newline,
        );
        assert_eq!(output.len(), 2);
        assert_eq!(
            output
                .iter()
                .map(|r| r.metadata.event_count())
                .sum::<usize>(),
            2
        );
        let mut decoded = Vec::new();
        for request in output {
            assert!(request.record.data.len() + request.record.key.len() <= RECORD_LIMIT);
            vector_common::decompression::CappedDecoder::gzip(request.record.data.as_ref())
                .into_reader()
                .read_to_end(&mut decoded)
                .unwrap();
        }
        assert_eq!(decoded, random);
    }
}
