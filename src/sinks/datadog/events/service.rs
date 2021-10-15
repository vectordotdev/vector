use crate::sinks::datadog::events::config::DatadogEventsConfig;
use crate::sinks::datadog::ApiKey;
use crate::sinks::util::encoding::{EncodingConfigWithDefault, TimestampFormat, EncodingConfiguration};
use crate::sinks::util::http::HttpSink;
use crate::sinks::util::{PartitionInnerBuffer, BoxedRawValue};
use crate::event::Event;
use serde_json::json;
use std::sync::Arc;
use http::Request;
use crate::internal_events::{DatadogEventsProcessed, DatadogEventsFieldInvalid};
use crate::config::log_schema;
use crate::event::PathComponent;

#[derive(Clone)]
pub struct DatadogEventsService {
    config: DatadogEventsConfig,
    uri: String,
    default_api_key: ApiKey,
    encoding: EncodingConfigWithDefault<()>,
}

impl DatadogEventsService {
    pub fn new(config: &DatadogEventsConfig) -> Self {
        let encoding = EncodingConfigWithDefault {
            // DataDog Event API allows only some fields, and refuses
            // to accept event if it contains any other field.
            only_fields: Some(
                [
                    "aggregation_key",
                    "alert_type",
                    "date_happened",
                    "device_name",
                    "host",
                    "priority",
                    "related_event_id",
                    "source_type_name",
                    "tags",
                    "text",
                    "title",
                ]
                    .iter()
                    .map(|field| vec![PathComponent::Key((*field).into())])
                    .collect(),
            ),
            // DataDog Event API requires unix timestamp.
            timestamp_format: Some(TimestampFormat::Unix),
            ..EncodingConfigWithDefault::default()
        };

        Self {
            default_api_key: Arc::from(config.default_api_key.clone()),
            uri: config.get_uri(),
            encoding,
            config: config.clone(),
        }
    }
}

#[async_trait::async_trait]
impl HttpSink for DatadogEventsService {
    type Input = PartitionInnerBuffer<serde_json::Value, ApiKey>;
    type Output = PartitionInnerBuffer<Vec<BoxedRawValue>, ApiKey>;

    fn encode_event(&self, mut event: Event) -> Option<Self::Input> {
        let log = event.as_mut_log();

        if !log.contains("title") {
            emit!(&DatadogEventsFieldInvalid { field: "title" });
            return None;
        }

        if !log.contains("text") {
            if let Some(message) = log.remove(log_schema().message_key()) {
                log.insert("text", message);
            } else {
                emit!(&DatadogEventsFieldInvalid {
                    field: log_schema().message_key()
                });
                return None;
            }
        }

        if !log.contains("host") {
            if let Some(host) = log.remove(log_schema().host_key()) {
                log.insert("host", host);
            }
        }

        if !log.contains("date_happened") {
            if let Some(timestamp) = log.remove(log_schema().timestamp_key()) {
                log.insert("date_happened", timestamp);
            }
        }

        if !log.contains("source_type_name") {
            if let Some(name) = log.remove(log_schema().source_type_key()) {
                log.insert("source_type_name", name);
            }
        }

        self.encoding.apply_rules(&mut event);

        let (fields, metadata) = event.into_log().into_parts();
        let json_event = json!(fields);
        let api_key = metadata
            .datadog_api_key()
            .as_ref()
            .unwrap_or(&self.default_api_key);

        Some(PartitionInnerBuffer::new(json_event, Arc::clone(api_key)))
    }

    async fn build_request(&self, events: Self::Output) -> crate::Result<Request<Vec<u8>>> {
        let (mut events, api_key) = events.into_parts();

        assert_eq!(events.len(), 1);
        let body = serde_json::to_vec(&events.pop().expect("One event"))?;

        emit!(&DatadogEventsProcessed {
            byte_size: body.len(),
        });

        Request::post(self.uri.as_str())
            .header("Content-Type", "application/json")
            .header("DD-API-KEY", &api_key[..])
            .header("Content-Length", body.len())
            .body(body)
            .map_err(Into::into)
    }
}
