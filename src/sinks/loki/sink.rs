use std::{collections::HashMap, num::NonZeroUsize};

use bytes::{Bytes, BytesMut};
use once_cell::sync::Lazy;
use regex::Regex;
use snafu::Snafu;
use tokio_util::codec::Encoder as _;
use vrl::path::parse_target_path;

use super::{
    config::{LokiConfig, OutOfOrderAction},
    event::{LokiBatchEncoder, LokiEvent, LokiRecord, PartitionKey},
    service::{LokiRequest, LokiRetryLogic, LokiService},
};
use crate::sinks::loki::event::LokiBatchEncoding;
use crate::{
    http::{get_http_scheme_from_uri, HttpClient},
    internal_events::{
        LokiEventUnlabeledError, LokiOutOfOrderEventDroppedError, LokiOutOfOrderEventRewritten,
        LokiTimestampNonParsableEventsDropped, SinkRequestBuildError,
    },
    sinks::prelude::*,
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
    #[snafu(display("Failed to build payload with error: {}", error))]
    Io { error: std::io::Error },
}

impl From<std::io::Error> for RequestBuildError {
    fn from(error: std::io::Error) -> RequestBuildError {
        RequestBuildError::Io { error }
    }
}

impl RequestBuilder<(PartitionKey, Vec<LokiRecord>)> for LokiRequestBuilder {
    type Metadata = (Option<String>, EventFinalizers);
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
    ) -> (Self::Metadata, RequestMetadataBuilder, Self::Events) {
        let (key, mut events) = input;

        let metadata_builder = RequestMetadataBuilder::from_events(&events);
        let finalizers = events.take_finalizers();

        ((key.tenant_id, finalizers), metadata_builder, events)
    }

    fn build_request(
        &self,
        loki_metadata: Self::Metadata,
        metadata: RequestMetadata,
        payload: EncodeResult<Self::Payload>,
    ) -> Self::Request {
        let (tenant_id, finalizers) = loki_metadata;
        let compression = self.compression;

        LokiRequest {
            compression,
            finalizers,
            payload: payload.into_payload(),
            tenant_id,
            metadata,
        }
    }
}

#[derive(Clone)]
pub(super) struct EventEncoder {
    key_partitioner: KeyPartitioner,
    transformer: Transformer,
    encoder: Encoder<()>,
    labels: HashMap<Template, Template>,
    remove_label_fields: bool,
    structured_metadata: HashMap<Template, Template>,
    remove_structured_metadata_fields: bool,
    remove_timestamp: bool,
}

impl EventEncoder {
    fn build_labels(&self, event: &Event) -> Vec<(String, String)> {
        let mut static_labels: HashMap<String, String> = HashMap::new();
        let mut dynamic_labels: HashMap<String, String> = HashMap::new();

        for (key_template, value_template) in self.labels.iter() {
            let key = key_template.render_string(event);
            let value = value_template.render_string(event);

            if key.is_err() || value.is_err() {
                if key.is_err() {
                    emit!(TemplateRenderingError {
                        field: Some(
                            format!(
                                "label_key \"{}\" with label_value \"{}\"",
                                key_template, value_template
                            )
                            .as_str()
                        ),
                        drop_event: false,
                        error: key.err().unwrap(),
                    });
                }
                if value.is_err() {
                    emit!(TemplateRenderingError {
                        field: Some(
                            format!(
                                "label_value \"{}\" with label_key \"{}\"",
                                value_template, key_template
                            )
                            .as_str()
                        ),
                        drop_event: false,
                        error: value.err().unwrap(),
                    });
                }
                continue;
            }

            let key_s = key.unwrap();
            let value_s = value.unwrap();

            if let Some(opening_prefix) = key_s.strip_suffix('*') {
                let output: Result<
                    serde_json::map::Map<String, serde_json::Value>,
                    serde_json::Error,
                > = serde_json::from_str(value_s.clone().as_str());

                if output.is_err() {
                    warn!(
                        "Failed to expand dynamic label. value: {}, err: {}",
                        value_s,
                        output.err().unwrap()
                    );
                    continue;
                }

                // key_* -> key_one, key_two, key_three
                // * -> one, two, three
                for (k, v) in output.unwrap() {
                    let key = slugify_text(format!("{}{}", opening_prefix, k));
                    let val = Value::from(v).to_string_lossy().into_owned();
                    if val == "<null>" {
                        warn!("Encountered \"null\" value for dynamic label. key: {}", key);
                        continue;
                    }
                    if let Some(prev) = dynamic_labels.insert(key.clone(), val.clone()) {
                        warn!(
                            "Encountered duplicated dynamic label. \
                                key: {}, value: {}, discarded value: {}",
                            key, val, prev
                        );
                    };
                }
            } else {
                static_labels.insert(key_s, value_s);
            }
        }

        for (k, v) in static_labels {
            if let Some(discarded_v) = dynamic_labels.insert(k.clone(), v.clone()) {
                warn!(
                    "Static label overrides dynamic label. \
                key: {}, value: {}, discarded value: {}",
                    k, v, discarded_v
                );
            };
        }

        Vec::from_iter(dynamic_labels)
    }

    fn remove_label_fields(&self, event: &mut Event) {
        if self.remove_label_fields {
            for template in self.labels.values() {
                if let Some(fields) = template.get_fields() {
                    for field in fields {
                        if let Ok(path) = parse_target_path(field.as_str()) {
                            event.as_mut_log().remove(&path);
                        }
                    }
                }
            }
        }
    }

    fn build_structured_metadata(&self, event: &Event) -> Vec<(String, String)> {
        let mut static_structured_metadata: HashMap<String, String> = HashMap::new();
        let mut dynamic_structured_metadata: HashMap<String, String> = HashMap::new();

        for (key_template, value_template) in self.structured_metadata.iter() {
            let key = key_template.render_string(event);
            let value = value_template.render_string(event);

            if key.is_err() || value.is_err() {
                if key.is_err() {
                    emit!(TemplateRenderingError {
                        field: Some(
                            format!(
                        "structured_metadata_key \"{}\" with structured_metadata_value \"{}\"",
                        key_template, value_template
                    )
                            .as_str()
                        ),
                        drop_event: false,
                        error: key.err().unwrap(),
                    });
                }
                if value.is_err() {
                    emit!(TemplateRenderingError {
                        field: Some(
                            format!(
                        "structured_metadata_value \"{}\" with structured_metadata_key \"{}\"",
                        value_template, key_template
                    )
                            .as_str()
                        ),
                        drop_event: false,
                        error: value.err().unwrap(),
                    });
                }
                continue;
            }

            let key_s = key.unwrap();
            let value_s = value.unwrap();

            if let Some(opening_prefix) = key_s.strip_suffix('*') {
                let output: Result<
                    serde_json::map::Map<String, serde_json::Value>,
                    serde_json::Error,
                > = serde_json::from_str(value_s.clone().as_str());

                if output.is_err() {
                    warn!(
                        "Failed to expand dynamic structured metadata. value: {}, err: {}",
                        value_s,
                        output.err().unwrap()
                    );
                    continue;
                }

                // key_* -> key_one, key_two, key_three
                // * -> one, two, three
                for (k, v) in output.unwrap() {
                    let key = slugify_text(format!("{}{}", opening_prefix, k));
                    let val = Value::from(v).to_string_lossy().into_owned();
                    if val == "<null>" {
                        warn!(
                            "Encountered \"null\" value for dynamic structured_metadata. key: {}",
                            key
                        );
                        continue;
                    }
                    if let Some(prev) = dynamic_structured_metadata.insert(key.clone(), val.clone())
                    {
                        warn!(
                            "Encountered duplicated dynamic structured_metadata. \
                        key: {}, value: {}, discarded value: {}",
                            key, val, prev
                        );
                    };
                }
            } else {
                static_structured_metadata.insert(key_s, value_s);
            }
        }

        for (k, v) in static_structured_metadata {
            if let Some(discarded_v) = dynamic_structured_metadata.insert(k.clone(), v.clone()) {
                warn!(
                    "Static label overrides dynamic label. \
        key: {}, value: {}, discarded value: {}",
                    k, v, discarded_v
                );
            };
        }

        Vec::from_iter(dynamic_structured_metadata)
    }

    fn remove_structured_metadata_fields(&self, event: &mut Event) {
        if self.remove_structured_metadata_fields {
            for template in self.structured_metadata.values() {
                if let Some(fields) = template.get_fields() {
                    for field in fields {
                        if let Ok(path) = parse_target_path(field.as_str()) {
                            event.as_mut_log().remove(&path);
                        }
                    }
                }
            }
        }
    }

    pub(super) fn encode_event(&mut self, mut event: Event) -> Option<LokiRecord> {
        let tenant_id = self.key_partitioner.partition(&event);
        let finalizers = event.take_finalizers();
        let json_byte_size = event.estimated_json_encoded_size_of();
        let mut labels: Vec<(String, String)> = self.build_labels(&event);
        self.remove_label_fields(&mut event);
        let structured_metadata: Vec<(String, String)> = self.build_structured_metadata(&event);
        self.remove_structured_metadata_fields(&mut event);

        let timestamp = match event.as_log().get_timestamp() {
            Some(Value::Timestamp(ts)) => match ts.timestamp_nanos_opt() {
                Some(timestamp) => timestamp,
                None => {
                    finalizers.update_status(EventStatus::Errored);
                    emit!(LokiTimestampNonParsableEventsDropped);
                    return None;
                }
            },
            _ => chrono::Utc::now()
                .timestamp_nanos_opt()
                .expect("Timestamp out of range"),
        };

        if self.remove_timestamp {
            event.as_mut_log().remove_timestamp();
        }

        let event_count_tags = event.get_tags();

        self.transformer.transform(&mut event);
        let mut bytes = BytesMut::new();
        self.encoder.encode(event, &mut bytes).ok();

        // If no labels are provided we set our own default
        // `{agent="vector"}` label. This can happen if the only
        // label is a templatable one but the event doesn't match.
        if labels.is_empty() {
            emit!(LokiEventUnlabeledError);
            labels = vec![("agent".to_string(), "vector".to_string())]
        }

        let partition = PartitionKey { tenant_id };

        Some(LokiRecord {
            labels,
            event: LokiEvent {
                timestamp,
                event: bytes.freeze(),
                structured_metadata: structured_metadata.clone(),
            },
            partition,
            finalizers,
            json_byte_size,
            event_count_tags,
        })
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
    fn size_of(&self) -> usize {
        self.inner.size_of()
    }

    fn allocated_bytes(&self) -> usize {
        self.inner.allocated_bytes()
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
    request_builder: LokiRequestBuilder,
    pub(super) encoder: EventEncoder,
    batch_settings: BatcherSettings,
    out_of_order_action: OutOfOrderAction,
    service: Svc<LokiService, LokiRetryLogic>,
    protocol: &'static str,
}

impl LokiSink {
    #[allow(clippy::missing_const_for_fn)] // const cannot run destructor
    pub fn new(config: LokiConfig, client: HttpClient) -> crate::Result<Self> {
        let compression = config.compression;

        // if Vector is configured to allow events with out of order timestamps, then we can
        // safely enable concurrency settings.
        //
        // For rewritten timestamps, we use a static concurrency of 1 to avoid out-of-order
        // timestamps across requests. We used to support concurrency across partitions (Loki
        // streams) but this was lost in #9506. Rather than try to re-add it, since Loki no longer
        // requires in-order processing for version >= 2.4, instead we just keep the static limit
        // of 1 for now.
        let request_limits = match config.out_of_order_action {
            OutOfOrderAction::Accept => config.request.into_settings(),
            OutOfOrderAction::Drop | OutOfOrderAction::RewriteTimestamp => {
                let mut settings = config.request.into_settings();
                settings.concurrency = Some(1);
                settings
            }
        };

        let protocol = get_http_scheme_from_uri(&config.endpoint.uri);
        let service = tower::ServiceBuilder::new()
            .settings(request_limits, LokiRetryLogic)
            .service(LokiService::new(
                client,
                config.endpoint,
                config.path,
                config.auth,
            )?);

        let transformer = config.encoding.transformer();
        let serializer = config.encoding.build()?;
        let encoder = Encoder::<()>::new(serializer);
        let batch_encoder = match config.compression {
            Compression::Snappy => LokiBatchEncoder(LokiBatchEncoding::Protobuf),
            _ => LokiBatchEncoder(LokiBatchEncoding::Json),
        };

        Ok(Self {
            request_builder: LokiRequestBuilder {
                compression,
                encoder: batch_encoder,
            },
            encoder: EventEncoder {
                key_partitioner: KeyPartitioner::new(config.tenant_id),
                transformer,
                encoder,
                labels: config.labels,
                structured_metadata: config.structured_metadata,
                remove_label_fields: config.remove_label_fields,
                remove_structured_metadata_fields: config.remove_structured_metadata_fields,
                remove_timestamp: config.remove_timestamp,
            },
            batch_settings: config.batch.into_batcher_settings()?,
            out_of_order_action: config.out_of_order_action,
            service,
            protocol,
        })
    }

    async fn run_inner(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        let mut encoder = self.encoder.clone();
        let mut filter = RecordFilter::new(self.out_of_order_action);

        // out_of_order_action's that require a complete ordering are limited to building 1 request
        // at a time
        let request_builder_concurrency = match self.out_of_order_action {
            OutOfOrderAction::Accept => default_request_builder_concurrency_limit(),
            OutOfOrderAction::Drop | OutOfOrderAction::RewriteTimestamp => {
                NonZeroUsize::new(1).expect("static")
            }
        };
        let batch_settings = self.batch_settings;

        input
            .map(|event| encoder.encode_event(event))
            .filter_map(|event| async { event })
            .map(|record| filter.filter_record(record))
            .batched_partitioned(RecordPartitioner, || batch_settings.as_byte_size_config())
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
                    emit!(LokiOutOfOrderEventDroppedError { count: batch.len() });
                    None
                }
            })
            .request_builder(request_builder_concurrency, self.request_builder)
            .filter_map(|request| async move {
                match request {
                    Err(error) => {
                        emit!(SinkRequestBuildError { error });
                        None
                    }
                    Ok(req) => Some(req),
                }
            })
            .into_driver(self.service)
            .protocol(self.protocol)
            .run()
            .await
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
    use std::{collections::HashMap, convert::TryFrom};

    use futures::stream::StreamExt;
    use vector_lib::codecs::JsonSerializerConfig;
    use vector_lib::event::{Event, LogEvent, ObjectMap, Value};
    use vector_lib::lookup::PathPrefix;

    use super::{EventEncoder, KeyPartitioner, RecordFilter};
    use crate::{
        codecs::Encoder, config::log_schema, sinks::loki::config::OutOfOrderAction,
        template::Template, test_util::random_lines,
    };

    #[test]
    fn encoder_no_labels() {
        let mut encoder = EventEncoder {
            key_partitioner: KeyPartitioner::new(None),
            transformer: Default::default(),
            encoder: Encoder::<()>::new(JsonSerializerConfig::default().build().into()),
            labels: HashMap::default(),
            structured_metadata: HashMap::default(),
            remove_label_fields: false,
            remove_structured_metadata_fields: false,
            remove_timestamp: false,
        };
        let mut event = Event::Log(LogEvent::from("hello world"));
        let log = event.as_mut_log();
        log.insert(
            (PathPrefix::Event, log_schema().timestamp_key().unwrap()),
            chrono::Utc::now(),
        );
        let record = encoder.encode_event(event).unwrap();
        assert!(String::from_utf8_lossy(&record.event.event)
            .contains(log_schema().timestamp_key().unwrap().to_string().as_str()));
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
        let mut encoder = EventEncoder {
            key_partitioner: KeyPartitioner::new(None),
            transformer: Default::default(),
            encoder: Encoder::<()>::new(JsonSerializerConfig::default().build().into()),
            labels,
            structured_metadata: HashMap::default(),
            remove_label_fields: false,
            remove_structured_metadata_fields: false,
            remove_timestamp: false,
        };
        let mut event = Event::Log(LogEvent::from("hello world"));
        let log = event.as_mut_log();
        log.insert(
            (PathPrefix::Event, log_schema().timestamp_key().unwrap()),
            chrono::Utc::now(),
        );
        log.insert("name", "foo");
        log.insert("value", "bar");

        let mut test_dict = ObjectMap::default();
        test_dict.insert("one".into(), Value::from("foo"));
        test_dict.insert("two".into(), Value::from("baz"));
        log.insert("dict", Value::from(test_dict));

        let record = encoder.encode_event(event).unwrap();
        assert!(String::from_utf8_lossy(&record.event.event)
            .contains(log_schema().timestamp_key().unwrap().to_string().as_str()));
        assert_eq!(record.labels.len(), 4);

        let labels: HashMap<String, String> = record.labels.into_iter().collect();
        assert_eq!(labels["static"], "value".to_string());
        assert_eq!(labels["foo"], "bar".to_string());
        assert_eq!(labels["test_key_one"], "foo".to_string());
        assert_eq!(labels["test_key_two"], "baz".to_string());
    }

    #[test]
    fn encoder_with_dynamic_labels() -> Result<(), serde_json::Error> {
        let mut labels = HashMap::default();
        labels.insert(
            Template::try_from("pod_labels_*").unwrap(),
            Template::try_from("{{ kubernetes.pod_labels }}").unwrap(),
        );
        labels.insert(
            Template::try_from("*").unwrap(),
            Template::try_from("{{ metadata }}").unwrap(),
        );
        labels.insert(
            Template::try_from("cluster_name").unwrap(),
            Template::try_from("static_cluster_name").unwrap(),
        );

        let mut encoder = EventEncoder {
            key_partitioner: KeyPartitioner::new(None),
            transformer: Default::default(),
            encoder: Encoder::<()>::new(JsonSerializerConfig::default().build().into()),
            labels,
            structured_metadata: HashMap::default(),
            remove_label_fields: false,
            remove_structured_metadata_fields: false,
            remove_timestamp: false,
        };

        let message = r#"
        {
        	"kubernetes": {
        		"pod_labels": {
        			"app": "web-server",
        			"name": "unicorn"
        		}
        	},
        	"metadata": {
        		"cluster_name": "operations",
        		"cluster_environment": "development",
        		"cluster_version": "1.2.3"
        	}
        }
        "#;
        let msg: ObjectMap = serde_json::from_str(message)?;
        let event = Event::Log(LogEvent::from(msg));
        let record = encoder.encode_event(event).unwrap();

        assert_eq!(record.labels.len(), 5);
        let labels: HashMap<String, String> = record.labels.into_iter().collect();
        assert_eq!(labels["pod_labels_app"], "web-server".to_string());
        assert_eq!(labels["pod_labels_name"], "unicorn".to_string());
        assert_eq!(labels["cluster_name"], "static_cluster_name".to_string());
        assert_eq!(labels["cluster_environment"], "development".to_string());
        assert_eq!(labels["cluster_version"], "1.2.3".to_string());
        Ok(())
    }

    #[test]
    fn encoder_with_colliding_dynamic_labels() -> Result<(), serde_json::Error> {
        let mut labels = HashMap::default();
        labels.insert(
            Template::try_from("l1_*").unwrap(),
            Template::try_from("{{ map1 }}").unwrap(),
        );
        labels.insert(
            Template::try_from("*").unwrap(),
            Template::try_from("{{ map2 }}").unwrap(),
        );

        let mut encoder = EventEncoder {
            key_partitioner: KeyPartitioner::new(None),
            transformer: Default::default(),
            encoder: Encoder::<()>::new(JsonSerializerConfig::default().build().into()),
            labels,
            structured_metadata: HashMap::default(),
            remove_label_fields: false,
            remove_structured_metadata_fields: false,
            remove_timestamp: false,
        };

        let message = r#"
        {
        	"map1": {
        		"key1": "val1"
        	},
        	"map2": {
        		"l1_key1": "val2"
        	}
        }
        "#;
        let msg: ObjectMap = serde_json::from_str(message)?;
        let event = Event::Log(LogEvent::from(msg));
        let record = encoder.encode_event(event).unwrap();

        assert_eq!(record.labels.len(), 1);
        let labels: HashMap<String, String> = record.labels.into_iter().collect();
        // EventEncoder.labels is type HashMap (unordered) -> both values can be valid
        assert!(["val1".to_string(), "val2".to_string()].contains(&labels["l1_key1"]));
        Ok(())
    }

    #[test]
    fn encoder_with_failing_dynamic_label_expansion() -> Result<(), serde_json::Error> {
        let mut labels = HashMap::default();
        labels.insert(
            Template::try_from("missing_*").unwrap(),
            Template::try_from("{{ map }}").unwrap(),
        );

        let mut encoder = EventEncoder {
            key_partitioner: KeyPartitioner::new(None),
            transformer: Default::default(),
            encoder: Encoder::<()>::new(JsonSerializerConfig::default().build().into()),
            labels,
            structured_metadata: HashMap::default(),
            remove_label_fields: false,
            remove_structured_metadata_fields: false,
            remove_timestamp: false,
        };

        let msg: ObjectMap = serde_json::from_str("{}")?;
        let event = Event::Log(LogEvent::from(msg));
        let record = encoder.encode_event(event).unwrap();

        assert_eq!(record.labels.len(), 1);
        let labels: HashMap<String, String> = record.labels.into_iter().collect();
        assert_eq!(labels["agent"], "vector".to_string());
        Ok(())
    }

    #[test]
    fn encoder_no_ts() {
        let mut encoder = EventEncoder {
            key_partitioner: KeyPartitioner::new(None),
            transformer: Default::default(),
            encoder: Encoder::<()>::new(JsonSerializerConfig::default().build().into()),
            labels: HashMap::default(),
            structured_metadata: HashMap::default(),
            remove_label_fields: false,
            remove_structured_metadata_fields: false,
            remove_timestamp: true,
        };
        let mut event = Event::Log(LogEvent::from("hello world"));
        let log = event.as_mut_log();
        log.insert(
            (PathPrefix::Event, log_schema().timestamp_key().unwrap()),
            chrono::Utc::now(),
        );
        let record = encoder.encode_event(event).unwrap();
        assert!(!String::from_utf8_lossy(&record.event.event)
            .contains(log_schema().timestamp_key().unwrap().to_string().as_str()));
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
        let mut encoder = EventEncoder {
            key_partitioner: KeyPartitioner::new(None),
            transformer: Default::default(),
            encoder: Encoder::<()>::new(JsonSerializerConfig::default().build().into()),
            labels,
            structured_metadata: HashMap::default(),
            remove_label_fields: true,
            remove_structured_metadata_fields: false,
            remove_timestamp: false,
        };
        let mut event = Event::Log(LogEvent::from("hello world"));
        let log = event.as_mut_log();
        log.insert(
            (PathPrefix::Event, log_schema().timestamp_key().unwrap()),
            chrono::Utc::now(),
        );
        log.insert("name", "foo");
        log.insert("value", "bar");
        let record = encoder.encode_event(event).unwrap();
        assert!(!String::from_utf8_lossy(&record.event.event).contains("value"));
    }

    #[test]
    fn encoder_with_structured_metadata() -> Result<(), serde_json::Error> {
        let mut structured_metadata = HashMap::default();
        structured_metadata.insert(
            Template::try_from("pod_labels_*").unwrap(),
            Template::try_from("{{ kubernetes.pod_labels }}").unwrap(),
        );
        structured_metadata.insert(
            Template::try_from("*").unwrap(),
            Template::try_from("{{ metadata }}").unwrap(),
        );
        structured_metadata.insert(
            Template::try_from("cluster_name").unwrap(),
            Template::try_from("static_cluster_name").unwrap(),
        );

        let mut encoder = EventEncoder {
            key_partitioner: KeyPartitioner::new(None),
            transformer: Default::default(),
            encoder: Encoder::<()>::new(JsonSerializerConfig::default().build().into()),
            labels: HashMap::default(),
            structured_metadata,
            remove_label_fields: false,
            remove_structured_metadata_fields: false,
            remove_timestamp: false,
        };

        let message = r#"
        {
        	"kubernetes": {
        		"pod_labels": {
        			"app": "web-server",
        			"name": "unicorn"
        		}
        	},
        	"metadata": {
        		"cluster_name": "operations",
        		"cluster_environment": "development",
        		"cluster_version": "1.2.3"
        	}
        }
        "#;
        let msg: ObjectMap = serde_json::from_str(message)?;
        let event = Event::Log(LogEvent::from(msg));
        let record = encoder.encode_event(event).unwrap();

        assert_eq!(record.event.structured_metadata.len(), 5);
        let structured_metadata: HashMap<String, String> =
            record.event.structured_metadata.into_iter().collect();
        assert_eq!(
            structured_metadata["pod_labels_app"],
            "web-server".to_string()
        );
        assert_eq!(
            structured_metadata["pod_labels_name"],
            "unicorn".to_string()
        );
        assert_eq!(
            structured_metadata["cluster_name"],
            "static_cluster_name".to_string()
        );
        assert_eq!(
            structured_metadata["cluster_environment"],
            "development".to_string()
        );
        assert_eq!(structured_metadata["cluster_version"], "1.2.3".to_string());
        Ok(())
    }

    #[tokio::test]
    async fn filter_encoder_drop() {
        let mut encoder = EventEncoder {
            key_partitioner: KeyPartitioner::new(None),
            transformer: Default::default(),
            encoder: Encoder::<()>::new(JsonSerializerConfig::default().build().into()),
            labels: HashMap::default(),
            structured_metadata: HashMap::default(),
            remove_label_fields: false,
            remove_structured_metadata_fields: false,
            remove_timestamp: false,
        };
        let base = chrono::Utc::now();
        let events = random_lines(100)
            .take(20)
            .map(|e| Event::Log(LogEvent::from(e)))
            .enumerate()
            .map(|(i, mut event)| {
                let log = event.as_mut_log();
                let ts = if i % 5 == 1 {
                    base
                } else {
                    base + chrono::Duration::seconds(i as i64)
                };
                log.insert(
                    (PathPrefix::Event, log_schema().timestamp_key().unwrap()),
                    ts,
                );
                event
            })
            .collect::<Vec<_>>();
        let mut filter = RecordFilter::new(OutOfOrderAction::Drop);
        let stream = futures::stream::iter(events)
            .map(|event| encoder.encode_event(event))
            .filter_map(|event| async { event })
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
