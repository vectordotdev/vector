use bytes::{Bytes, BytesMut};
use std::collections::HashMap;
use tokio_util::codec::Encoder as _;
use vector_core::{config::LogSchema, ByteSizeOf};

use crate::{
    codecs::{Encoder, Transformer},
    event::{Event, Finalizable, Value},
    internal_events::PulsarPropertyExtractionError,
    sinks::pulsar::service::{PulsarRequest, PulsarRequestMetadata},
    template::Template,
};

pub struct PulsarRequestBuilder {
    pub key_field: Option<String>,
    pub properties_key: Option<String>,
    pub topic_template: Template,
    pub transformer: Transformer,
    pub encoder: Encoder<()>,
    pub log_schema: &'static LogSchema,
}

impl PulsarRequestBuilder {
    pub fn build_request(&mut self, mut event: Event) -> Option<PulsarRequest> {
        let topic = self.topic_template.render_string(&event).ok()?;
        let metadata = PulsarRequestMetadata {
            finalizers: event.take_finalizers(),
            key: get_key(&event, &self.key_field),
            timestamp_millis: get_timestamp_millis(&event, self.log_schema),
            properties: get_properties(&event, &self.properties_key),
            topic,
        };
        let event_byte_size = event.size_of();
        self.transformer.transform(&mut event);
        let mut body = BytesMut::new();
        self.encoder.encode(event, &mut body).ok()?;
        let body = body.freeze();
        Some(PulsarRequest {
            body,
            metadata,
            event_byte_size,
        })
    }
}

fn get_key(event: &Event, key_field: &Option<String>) -> Option<Bytes> {
    key_field.as_ref().and_then(|key_field| match event {
        Event::Log(log) => log
            .get(key_field.as_str())
            .map(|value| value.coerce_to_bytes()),
        Event::Metric(metric) => metric
            .tags()
            .and_then(|tags| tags.get(key_field))
            .map(|value| value.clone().into()),
        _ => None,
    })
}

fn get_timestamp_millis(event: &Event, log_schema: &'static LogSchema) -> Option<i64> {
    match &event {
        Event::Log(log) => log
            .get(log_schema.timestamp_key())
            .and_then(|v| v.as_timestamp())
            .copied(),
        Event::Metric(metric) => metric.timestamp(),
        _ => None,
    }
    .map(|ts| ts.timestamp_millis())
}

fn get_properties(
    event: &Event,
    properties_key: &Option<String>,
) -> Option<HashMap<String, Bytes>> {
    properties_key.as_ref().and_then(|properties_key| {
        if let Event::Log(log) = event {
            if let Some(properties) = log.get(properties_key.as_str()) {
                match properties {
                    Value::Object(headers_map) => {
                        let mut property_map = HashMap::new();
                        for (key, value) in headers_map {
                            if let Value::Bytes(value_bytes) = value {
                                property_map.insert(key.clone(), value_bytes.clone());
                            } else {
                                emit!(PulsarPropertyExtractionError {
                                    property_field: properties_key
                                });
                            }
                        }
                        return Some(property_map);
                    }
                    _ => {
                        emit!(PulsarPropertyExtractionError {
                            property_field: properties_key
                        });
                    }
                }
            }
        }
        None
    })
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use bytes::Bytes;

    use super::*;
    use crate::event::LogEvent;

    #[test]
    fn pulsar_get_headers() {
        let properties_key = "properties";
        let mut property_values = BTreeMap::new();
        property_values.insert("a-key".to_string(), Value::Bytes(Bytes::from("a-value")));
        property_values.insert("b-key".to_string(), Value::Bytes(Bytes::from("b-value")));

        let mut event = Event::Log(LogEvent::from("hello"));
        event.as_mut_log().insert(properties_key, property_values);

        let properties = get_properties(&event, &Some(properties_key.to_string())).unwrap();
        assert_eq!(properties.get("a-key").unwrap(), "a-value".as_bytes());
        assert_eq!(properties.get("b-key").unwrap(), "b-value".as_bytes());
    }
}
