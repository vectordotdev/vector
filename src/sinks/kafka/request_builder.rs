use crate::event::{Event, Finalizable, Value};
use crate::internal_events::KafkaHeaderExtractionFailed;
use crate::sinks::kafka::encoder::Encoding;
use crate::sinks::kafka::service::{KafkaRequest, KafkaRequestMetadata};
use crate::sinks::util::encoding::{Encoder, EncodingConfig};
use crate::template::Template;
use bytes::Bytes;
use rdkafka::message::OwnedHeaders;
use vector_core::config::LogSchema;

pub struct KafkaRequestBuilder {
    pub key_field: Option<String>,
    pub headers_field: Option<String>,
    pub topic_template: Template,
    pub encoder: EncodingConfig<Encoding>,
    pub log_schema: &'static LogSchema,
}

impl KafkaRequestBuilder {
    pub fn build_request(&self, mut event: Event) -> Option<KafkaRequest> {
        let topic = self.topic_template.render_string(&event).ok()?;
        let metadata = KafkaRequestMetadata {
            finalizers: event.take_finalizers(),
            key: get_key(&event, &self.key_field),
            timestamp_millis: get_timestamp_millis(&event, self.log_schema),
            headers: get_headers(&event, &self.headers_field),
            topic,
        };
        let mut body = vec![];
        self.encoder.encode_input(event, &mut body).ok()?;

        Some(KafkaRequest { body, metadata })
    }
}

fn get_key(event: &Event, key_field: &Option<String>) -> Option<Bytes> {
    key_field.as_ref().and_then(|key_field| match event {
        Event::Log(log) => log.get(key_field).map(|value| value.as_bytes()),
        Event::Metric(metric) => metric
            .tags()
            .and_then(|tags| tags.get(key_field))
            .map(|value| value.clone().into()),
    })
}

fn get_timestamp_millis(event: &Event, log_schema: &'static LogSchema) -> Option<i64> {
    match &event {
        Event::Log(log) => log
            .get(log_schema.timestamp_key())
            .and_then(|v| v.as_timestamp())
            .copied(),
        Event::Metric(metric) => metric.timestamp(),
    }
    .map(|ts| ts.timestamp_millis())
}

fn get_headers(event: &Event, headers_field: &Option<String>) -> Option<OwnedHeaders> {
    headers_field.as_ref().and_then(|headers_field| {
        if let Event::Log(log) = event {
            if let Some(headers) = log.get(headers_field) {
                match headers {
                    Value::Map(headers_map) => {
                        let mut owned_headers = OwnedHeaders::new_with_capacity(headers_map.len());
                        for (key, value) in headers_map {
                            if let Value::Bytes(value_bytes) = value {
                                owned_headers = owned_headers.add(key, value_bytes.as_ref());
                            } else {
                                emit!(&KafkaHeaderExtractionFailed {
                                    header_field: headers_field
                                });
                            }
                        }
                        return Some(owned_headers);
                    }
                    _ => {
                        emit!(&KafkaHeaderExtractionFailed {
                            header_field: headers_field
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
    use super::*;
    use bytes::Bytes;
    use rdkafka::message::Headers;
    use std::collections::BTreeMap;

    #[test]
    fn kafka_get_headers() {
        let headers_key = "headers";
        let mut header_values = BTreeMap::new();
        header_values.insert("a-key".to_string(), Value::Bytes(Bytes::from("a-value")));
        header_values.insert("b-key".to_string(), Value::Bytes(Bytes::from("b-value")));

        let mut event = Event::from("hello");
        event.as_mut_log().insert(headers_key, header_values);

        let headers = get_headers(&event, &Some(headers_key.to_string())).unwrap();
        assert_eq!(headers.get(0).unwrap().0, "a-key");
        assert_eq!(headers.get(0).unwrap().1, "a-value".as_bytes());
        assert_eq!(headers.get(1).unwrap().0, "b-key");
        assert_eq!(headers.get(1).unwrap().1, "b-value".as_bytes());
    }
}
