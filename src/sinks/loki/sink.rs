use once_cell::sync::Lazy;
use std::{collections::HashMap, num::NonZeroUsize};

use bytes::Bytes;
use futures::{stream::BoxStream, StreamExt};
use regex::Regex;
use snafu::Snafu;
use vector_common::encode_logfmt;
use vector_core::{
    buffers::Acker,
    event::{self, Event, EventFinalizers, Finalizable, Value},
    partition::Partitioner,
    sink::StreamSink,
    stream::BatcherSettings,
    ByteSizeOf,
};

use super::{
    config::{Encoding, LokiConfig, OutOfOrderAction},
    event::{LokiBatchEncoder, LokiEvent, LokiRecord, PartitionKey},
    service::{LokiRequest, LokiRetryLogic, LokiService},
};
use crate::{
    config::{log_schema, SinkContext},
    http::HttpClient,
    internal_events::{
        LokiEventUnlabeled, LokiOutOfOrderEventDropped, LokiOutOfOrderEventRewritten,
        TemplateRenderingError,
    },
    sinks::util::{
        builder::SinkBuilderExt,
        encoding::{EncodingConfig, EncodingConfiguration},
        service::{ServiceBuilderExt, Svc},
        Compression, RequestBuilder,
    },
    template::Template,
};

#[derive(Clone)]
pub struct KeyPartitioner(Option<Template>);

impl KeyPartitioner {
    pub const fn new(template: Option<Template>) -> Self {
        Self(template)
    }
}

impl Partitioner for KeyPartitioner {
    type Item = Event;
    type Key = Option<String>;

    fn partition(&self, item: &Self::Item) -> Self::Key {
        self.0.as_ref().and_then(|t| {
            t.render_string(item)
                .map_err(|error| {
                    emit!(TemplateRenderingError {
                        error,
                        field: Some("tenant_id"),
                        drop_event: false,
                    })
                })
                .ok()
        })
    }
}

#[derive(Default)]
struct RecordPartitioner;

impl Partitioner for RecordPartitioner {
    type Item = Option<FilteredRecord>;
    type Key = Option<PartitionKey>;

    fn partition(&self, item: &Self::Item) -> Self::Key {
        item.as_ref().map(|inner| inner.partition())
    }
}

#[derive(Clone)]
pub struct LokiRequestBuilder {
    compression: Compression,
    encoder: LokiBatchEncoder,
}

#[derive(Debug, Snafu)]
pub enum RequestBuildError {
    #[snafu(display("Encoded payload is greater than the max limit."))]
    PayloadTooBig,
    #[snafu(display("Failed to build payload with error: {}", error))]
    Io { error: std::io::Error },
}

impl From<std::io::Error> for RequestBuildError {
    fn from(error: std::io::Error) -> RequestBuildError {
        RequestBuildError::Io { error }
    }
}

impl RequestBuilder<(PartitionKey, Vec<LokiRecord>)> for LokiRequestBuilder {
    type Metadata = (Option<String>, usize, EventFinalizers, usize);
    type Events = Vec<LokiRecord>;
    type Encoder = LokiBatchEncoder;
    type Payload = Bytes;
    type Request = LokiRequest;
    type Error = RequestBuildError;

    fn compression(&self) -> Compression {
        self.compression
    }

    fn encoder(&self) -> &Self::Encoder {
        &self.encoder
    }

    fn split_input(
        &self,
        input: (PartitionKey, Vec<LokiRecord>),
    ) -> (Self::Metadata, Self::Events) {
        let (key, mut events) = input;
        let batch_size = events.len();
        let events_byte_size = events.size_of();
        let finalizers = events
            .iter_mut()
            .fold(EventFinalizers::default(), |mut acc, x| {
                acc.merge(x.take_finalizers());
                acc
            });

        (
            (key.tenant_id, batch_size, finalizers, events_byte_size),
            events,
        )
    }

    fn build_request(&self, metadata: Self::Metadata, payload: Self::Payload) -> Self::Request {
        let (tenant_id, batch_size, finalizers, events_byte_size) = metadata;
        let compression = self.compression();

        LokiRequest {
            compression,
            batch_size,
            finalizers,
            payload,
            tenant_id,
            events_byte_size,
        }
    }
}

#[derive(Clone)]
pub(super) struct EventEncoder {
    key_partitioner: KeyPartitioner,
    encoding: EncodingConfig<Encoding>,
    labels: HashMap<Template, Template>,
    remove_label_fields: bool,
    remove_timestamp: bool,
}

impl EventEncoder {
    fn build_labels(&self, event: &Event) -> Vec<(String, String)> {
        let mut vec: Vec<(String, String)> = Vec::new();

        for (key_template, value_template) in self.labels.iter() {
            if let (Ok(key), Ok(value)) = (
                key_template.render_string(event),
                value_template.render_string(event),
            ) {
                if let Some(opening_prefix) = key.strip_suffix('*') {
                    let output: Result<serde_json::map::Map<String, serde_json::Value>, _> =
                        serde_json::from_str(value.as_str());

                    if let Ok(output) = output {
                        // key_* -> key_one, key_two, key_three
                        for (k, v) in output {
                            vec.push((
                                slugify_text(format!("{}{}", opening_prefix, k)),
                                Value::from(v).to_string_lossy(),
                            ))
                        }
                    }
                } else {
                    vec.push((key, value));
                }
            }
        }
        vec
    }

    fn remove_label_fields(&self, event: &mut Event) {
        if self.remove_label_fields {
            for template in self.labels.values() {
                if let Some(fields) = template.get_fields() {
                    for field in fields {
                        event.as_mut_log().remove(field.as_str());
                    }
                }
            }
        }
    }

    pub(super) fn encode_event(&self, mut event: Event) -> LokiRecord {
        let tenant_id = self.key_partitioner.partition(&event);
        let finalizers = event.take_finalizers();
        let mut labels = self.build_labels(&event);
        self.remove_label_fields(&mut event);

        let schema = log_schema();
        let timestamp_key = schema.timestamp_key();
        let timestamp = match event.as_log().get(timestamp_key) {
            Some(event::Value::Timestamp(ts)) => ts.timestamp_nanos(),
            _ => chrono::Utc::now().timestamp_nanos(),
        };

        if self.remove_timestamp {
            event.as_mut_log().remove(timestamp_key);
        }

        self.encoding.apply_rules(&mut event);
        let log = event.into_log();
        let event = match &self.encoding.codec() {
            Encoding::Json => {
                serde_json::to_string(&log).expect("json encoding should never fail.")
            }

            Encoding::Text => log
                .get(schema.message_key())
                .map(Value::to_string_lossy)
                .unwrap_or_default(),

            Encoding::Logfmt => {
                encode_logfmt::to_string(log.as_map()).expect("Logfmt encoding should never fail.")
            }
        };

        // If no labels are provided we set our own default
        // `{agent="vector"}` label. This can happen if the only
        // label is a templatable one but the event doesn't match.
        if labels.is_empty() {
            emit!(LokiEventUnlabeled);
            labels = vec![("agent".to_string(), "vector".to_string())]
        }

        let partition = PartitionKey::new(tenant_id, &mut labels);

        LokiRecord {
            labels,
            event: LokiEvent { timestamp, event },
            partition,
            finalizers,
        }
    }
}

struct FilteredRecord {
    pub rewritten: bool,
    pub inner: LokiRecord,
}

impl FilteredRecord {
    pub const fn rewritten(inner: LokiRecord) -> Self {
        Self {
            rewritten: true,
            inner,
        }
    }

    pub const fn valid(inner: LokiRecord) -> Self {
        Self {
            rewritten: false,
            inner,
        }
    }

    pub fn partition(&self) -> PartitionKey {
        self.inner.partition.clone()
    }
}

impl ByteSizeOf for FilteredRecord {
    fn allocated_bytes(&self) -> usize {
        self.inner.allocated_bytes()
    }

    fn size_of(&self) -> usize {
        self.inner.size_of()
    }
}

struct RecordFilter {
    timestamps: HashMap<PartitionKey, i64>,
    out_of_order_action: OutOfOrderAction,
}

impl RecordFilter {
    fn new(out_of_order_action: OutOfOrderAction) -> Self {
        Self {
            timestamps: HashMap::new(),
            out_of_order_action,
        }
    }
}

impl RecordFilter {
    pub fn filter_record(&mut self, mut record: LokiRecord) -> Option<FilteredRecord> {
        if let Some(latest) = self.timestamps.get_mut(&record.partition) {
            if record.event.timestamp < *latest {
                match self.out_of_order_action {
                    OutOfOrderAction::Drop => None,
                    OutOfOrderAction::RewriteTimestamp => {
                        record.event.timestamp = *latest;
                        Some(FilteredRecord::rewritten(record))
                    }
                    OutOfOrderAction::Accept => Some(FilteredRecord::valid(record)),
                }
            } else {
                *latest = record.event.timestamp;
                Some(FilteredRecord::valid(record))
            }
        } else {
            self.timestamps
                .insert(record.partition.clone(), record.event.timestamp);
            Some(FilteredRecord::valid(record))
        }
    }
}

pub struct LokiSink {
    acker: Acker,
    request_builder: LokiRequestBuilder,
    pub(super) encoder: EventEncoder,
    batch_settings: BatcherSettings,
    out_of_order_action: OutOfOrderAction,
    service: Svc<LokiService, LokiRetryLogic>,
}

impl LokiSink {
    #[allow(clippy::missing_const_for_fn)] // const cannot run destructor
    pub fn new(config: LokiConfig, client: HttpClient, cx: SinkContext) -> crate::Result<Self> {
        let compression = config.compression;

        // if Vector is configured to allow events with out of order timestamps, then then we can
        // safely enable concurrency settings.
        //
        // For rewritten timestamps, we use a static concurrency of 1 to avoid out-of-order
        // timestamps across requests. We used to support concurrency across partitions (Loki
        // streams) but this was lost in #9506. Rather than try to re-add it, since Loki no longer
        // requires in-order processing for version >= 2.4, instead we just keep the static limit
        // of 1 for now.
        let request_limits = match config.out_of_order_action {
            OutOfOrderAction::Accept => config.request.unwrap_with(&Default::default()),
            OutOfOrderAction::Drop | OutOfOrderAction::RewriteTimestamp => {
                let mut settings = config.request.unwrap_with(&Default::default());
                settings.concurrency = Some(1);
                settings
            }
        };

        let service = tower::ServiceBuilder::new()
            .settings(request_limits, LokiRetryLogic)
            .service(LokiService::new(client, config.endpoint, config.auth)?);

        Ok(Self {
            acker: cx.acker(),
            request_builder: LokiRequestBuilder {
                compression,
                encoder: Default::default(),
            },
            encoder: EventEncoder {
                key_partitioner: KeyPartitioner::new(config.tenant_id),
                encoding: config.encoding,
                labels: config.labels,
                remove_label_fields: config.remove_label_fields,
                remove_timestamp: config.remove_timestamp,
            },
            batch_settings: config.batch.into_batcher_settings()?,
            out_of_order_action: config.out_of_order_action,
            service,
        })
    }

    async fn run_inner(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        let encoder = self.encoder.clone();
        let mut filter = RecordFilter::new(self.out_of_order_action);

        // out_of_order_action's that require a complete ordering are limited to building 1 request
        // at a time
        let request_builder_concurrency = match self.out_of_order_action {
            OutOfOrderAction::Accept => NonZeroUsize::new(50).expect("static"),
            OutOfOrderAction::Drop | OutOfOrderAction::RewriteTimestamp => {
                NonZeroUsize::new(1).expect("static")
            }
        };

        let sink = input
            .map(|event| encoder.encode_event(event))
            .map(|record| filter.filter_record(record))
            .batched_partitioned(RecordPartitioner::default(), self.batch_settings)
            .filter_map(|(partition, batch)| async {
                if let Some(partition) = partition {
                    let mut count: usize = 0;
                    let result = batch
                        .into_iter()
                        .flatten()
                        .map(|event| {
                            if event.rewritten {
                                count += 1;
                            }
                            event.inner
                        })
                        .collect::<Vec<_>>();
                    if count > 0 {
                        emit!(LokiOutOfOrderEventRewritten { count });
                    }
                    Some((partition, result))
                } else {
                    emit!(LokiOutOfOrderEventDropped { count: batch.len() });
                    None
                }
            })
            .request_builder(Some(request_builder_concurrency), self.request_builder)
            .filter_map(|request| async move {
                match request {
                    Err(e) => {
                        error!("Failed to build Loki request: {:?}.", e);
                        None
                    }
                    Ok(req) => Some(req),
                }
            })
            .into_driver(self.service, self.acker);

        sink.run().await
    }
}

#[async_trait::async_trait]
impl StreamSink<Event> for LokiSink {
    async fn run(mut self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        self.run_inner(input).await
    }
}

static RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"[^0-9A-Za-z_]").unwrap());

fn slugify_text(input: String) -> String {
    let result = RE.replace_all(&input, "_");
    result.to_lowercase()
}

#[cfg(test)]
mod tests {
    use std::{
        collections::{BTreeMap, HashMap},
        convert::TryFrom,
    };

    use futures::stream::StreamExt;
    use vector_core::event::{Event, Value};

    use super::{EventEncoder, KeyPartitioner, RecordFilter};
    use crate::{
        config::log_schema,
        sinks::{
            loki::config::{Encoding, OutOfOrderAction},
            util::encoding::EncodingConfig,
        },
        template::Template,
        test_util::random_lines,
    };

    #[test]
    fn encoder_no_labels() {
        let encoder = EventEncoder {
            key_partitioner: KeyPartitioner::new(None),
            encoding: EncodingConfig::from(Encoding::Json),
            labels: HashMap::default(),
            remove_label_fields: false,
            remove_timestamp: false,
        };
        let mut event = Event::from("hello world");
        let log = event.as_mut_log();
        log.insert(log_schema().timestamp_key(), chrono::Utc::now());
        let record = encoder.encode_event(event);
        assert!(record.event.event.contains(log_schema().timestamp_key()));
        assert_eq!(record.labels.len(), 1);
        assert_eq!(
            record.labels[0],
            ("agent".to_string(), "vector".to_string())
        );
    }

    #[test]
    fn encoder_with_labels() {
        let mut labels = HashMap::default();
        labels.insert(
            Template::try_from("static").unwrap(),
            Template::try_from("value").unwrap(),
        );
        labels.insert(
            Template::try_from("{{ name }}").unwrap(),
            Template::try_from("{{ value }}").unwrap(),
        );
        labels.insert(
            Template::try_from("test_key_*").unwrap(),
            Template::try_from("{{ dict }}").unwrap(),
        );
        labels.insert(
            Template::try_from("going_to_fail_*").unwrap(),
            Template::try_from("{{ value }}").unwrap(),
        );
        let encoder = EventEncoder {
            key_partitioner: KeyPartitioner::new(None),
            encoding: EncodingConfig::from(Encoding::Json),
            labels,
            remove_label_fields: false,
            remove_timestamp: false,
        };
        let mut event = Event::from("hello world");
        let log = event.as_mut_log();
        log.insert(log_schema().timestamp_key(), chrono::Utc::now());
        log.insert("name", "foo");
        log.insert("value", "bar");

        let mut test_dict = BTreeMap::default();
        test_dict.insert("one".to_string(), Value::from("foo"));
        test_dict.insert("two".to_string(), Value::from("baz"));
        log.insert("dict", Value::from(test_dict));

        let record = encoder.encode_event(event);
        assert!(record.event.event.contains(log_schema().timestamp_key()));
        assert_eq!(record.labels.len(), 4);

        let labels: HashMap<String, String> = record.labels.into_iter().collect();
        assert_eq!(labels["static"], "value".to_string());
        assert_eq!(labels["foo"], "bar".to_string());
        assert_eq!(labels["test_key_one"], "foo".to_string());
        assert_eq!(labels["test_key_two"], "baz".to_string());
    }

    #[test]
    fn encoder_no_ts() {
        let encoder = EventEncoder {
            key_partitioner: KeyPartitioner::new(None),
            encoding: EncodingConfig::from(Encoding::Json),
            labels: HashMap::default(),
            remove_label_fields: false,
            remove_timestamp: true,
        };
        let mut event = Event::from("hello world");
        let log = event.as_mut_log();
        log.insert(log_schema().timestamp_key(), chrono::Utc::now());
        let record = encoder.encode_event(event);
        assert!(!record.event.event.contains(log_schema().timestamp_key()));
    }

    #[test]
    fn encoder_no_record_labels() {
        let mut labels = HashMap::default();
        labels.insert(
            Template::try_from("static").unwrap(),
            Template::try_from("value").unwrap(),
        );
        labels.insert(
            Template::try_from("{{ name }}").unwrap(),
            Template::try_from("{{ value }}").unwrap(),
        );
        let encoder = EventEncoder {
            key_partitioner: KeyPartitioner::new(None),
            encoding: EncodingConfig::from(Encoding::Json),
            labels,
            remove_label_fields: true,
            remove_timestamp: false,
        };
        let mut event = Event::from("hello world");
        let log = event.as_mut_log();
        log.insert(log_schema().timestamp_key(), chrono::Utc::now());
        log.insert("name", "foo");
        log.insert("value", "bar");
        let record = encoder.encode_event(event);
        assert!(!record.event.event.contains("value"));
    }

    #[tokio::test]
    async fn filter_encoder_drop() {
        let encoder = EventEncoder {
            key_partitioner: KeyPartitioner::new(None),
            encoding: EncodingConfig::from(Encoding::Json),
            labels: HashMap::default(),
            remove_label_fields: false,
            remove_timestamp: false,
        };
        let base = chrono::Utc::now();
        let events = random_lines(100)
            .take(20)
            .map(Event::from)
            .enumerate()
            .map(|(i, mut event)| {
                let log = event.as_mut_log();
                let ts = if i % 5 == 1 {
                    base
                } else {
                    base + chrono::Duration::seconds(i as i64)
                };
                log.insert(log_schema().timestamp_key(), ts);
                event
            })
            .collect::<Vec<_>>();
        let mut filter = RecordFilter::new(OutOfOrderAction::Drop);
        let stream = futures::stream::iter(events)
            .map(|event| encoder.encode_event(event))
            .filter_map(|event| {
                let res = filter.filter_record(event);
                async { res }
            });
        tokio::pin!(stream);
        let mut result = Vec::new();
        while let Some(item) = stream.next().await {
            result.push(item);
        }
        assert_eq!(result.len(), 17);
    }
}
