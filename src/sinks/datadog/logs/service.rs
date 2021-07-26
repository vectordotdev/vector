use crate::sinks::datadog::logs::DatadogLogsConfig;
use crate::sinks::datadog::ApiKey;
use crate::sinks::util::encoding::EncodingConfiguration;
use crate::sinks::util::http::HttpSink;
use crate::sinks::util::{BoxedRawValue, EncodedEvent, PartitionInnerBuffer};
use crate::{config::log_schema, internal_events::DatadogLogEventProcessed};
use http::Request;
use serde_json::json;
use std::sync::Arc;
use vector_core::event::Event;

#[derive(Clone)]
pub(crate) struct DatadogLogsJsonService {
    pub(crate) config: DatadogLogsConfig,
    // Used to store the complete URI and avoid calling `get_uri` for each request
    pub(crate) uri: String,
    pub(crate) default_api_key: ApiKey,
}

#[async_trait::async_trait]
impl HttpSink for DatadogLogsJsonService {
    type Input = PartitionInnerBuffer<serde_json::Value, ApiKey>;
    type Output = PartitionInnerBuffer<Vec<BoxedRawValue>, ApiKey>;

    fn encode_event(&self, mut event: Event) -> Option<EncodedEvent<Self::Input>> {
        let log = event.as_mut_log();

        if let Some(message) = log.remove(log_schema().message_key()) {
            log.insert("message", message);
        }

        if let Some(timestamp) = log.remove(log_schema().timestamp_key()) {
            log.insert("date", timestamp);
        }

        if let Some(host) = log.remove(log_schema().host_key()) {
            log.insert("host", host);
        }

        self.config.encoding.apply_rules(&mut event);

        let (fields, metadata) = event.into_log().into_parts();
        let json_event = json!(fields);
        let api_key = metadata
            .datadog_api_key()
            .as_ref()
            .unwrap_or(&self.default_api_key);

        Some(EncodedEvent {
            item: PartitionInnerBuffer::new(json_event, Arc::clone(api_key)),
            metadata: Some(metadata),
        })
    }

    async fn build_request(&self, events: Self::Output) -> crate::Result<Request<Vec<u8>>> {
        let (events, api_key) = events.into_parts();

        let body = serde_json::to_vec(&events)?;
        // check the number of events to ignore health-check requests
        if !events.is_empty() {
            emit!(DatadogLogEventProcessed {
                byte_size: body.len(),
                count: events.len(),
            });
        }
        self.config
            .build_request(self.uri.as_str(), &api_key[..], "application/json", body)
    }
}
