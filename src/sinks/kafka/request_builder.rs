use std::num::NonZeroUsize;

use bytes::{Bytes, BytesMut};
use rdkafka::message::{Header, OwnedHeaders};
use tokio_util::codec::Encoder as _;

use crate::{
    codecs::{Encoder, Transformer},
    event::{Event, Finalizable, Value},
    internal_events::{KafkaHeaderExtractionError, TemplateRenderingError},
    sinks::{
        kafka::service::{KafkaRequest, KafkaRequestMetadata},
        util::metadata::RequestMetadataBuilder,
    },
    template::Template,
};

pub struct KafkaRequestBuilder {
    pub key_field: Option<String>,
    pub headers_key: Option<String>,
    pub topic_template: Template,
    pub transformer: Transformer,
    pub encoder: Encoder<()>,
}

impl KafkaRequestBuilder {
    pub fn build_request(&mut self, mut event: Event) -> Option<KafkaRequest> {
        let topic = self
            .topic_template
            .render_string(&event)
            .map_err(|error| {
                emit!(TemplateRenderingError {
                    field: None,
                    drop_event: true,
                    error,
                });
            })
            .ok()?;

        let metadata_builder = RequestMetadataBuilder::from_events(&event);

        let metadata = KafkaRequestMetadata {
            finalizers: event.take_finalizers(),
            key: get_key(&event, &self.key_field),
            timestamp_millis: get_timestamp_millis(&event),
            headers: get_headers(&event, &self.headers_key),
            topic,
        };
        self.transformer.transform(&mut event);
        let mut body = BytesMut::new();
        self.encoder.encode(event, &mut body).ok()?;
        let body = body.freeze();

        let bytes_len = NonZeroUsize::new(body.len()).expect("payload should never be zero length");
        let request_metadata = metadata_builder.with_request_size(bytes_len);

        Some(KafkaRequest {
            body,
            metadata,
            request_metadata,
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
            .map(|value| value.to_owned().into()),
        _ => None,
    })
}

fn get_timestamp_millis(event: &Event) -> Option<i64> {
    match &event {
        Event::Log(log) => log.get_timestamp().and_then(|v| v.as_timestamp()).copied(),
        Event::Metric(metric) => metric.timestamp(),
        _ => None,
    }
    .map(|ts| ts.timestamp_millis())
}

fn get_headers(event: &Event, headers_key: &Option<String>) -> Option<OwnedHeaders> {
    headers_key.as_ref().and_then(|headers_key| {
        if let Event::Log(log) = event {
            if let Some(headers) = log.get(headers_key.as_str()) {
                match headers {
                    Value::Object(headers_map) => {
                        let mut owned_headers = OwnedHeaders::new_with_capacity(headers_map.len());
                        for (key, value) in headers_map {
                            if let Value::Bytes(value_bytes) = value {
                                owned_headers = owned_headers.insert(Header {
                                    key,
                                    value: Some(value_bytes.as_ref()),
                                });
                            } else {
                                emit!(KafkaHeaderExtractionError {
                                    header_field: headers_key
                                });
                            }
                        }
                        return Some(owned_headers);
                    }
                    _ => {
                        emit!(KafkaHeaderExtractionError {
                            header_field: headers_key
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
    use rdkafka::message::Headers;

    use super::*;
    use crate::event::LogEvent;

    #[test]
    fn kafka_get_headers() {
        let headers_key = "headers";
        let mut header_values = BTreeMap::new();
        header_values.insert("a-key".to_string(), Value::Bytes(Bytes::from("a-value")));
        header_values.insert("b-key".to_string(), Value::Bytes(Bytes::from("b-value")));

        let mut event = Event::Log(LogEvent::from("hello"));
        event.as_mut_log().insert(headers_key, header_values);

        let headers = get_headers(&event, &Some(headers_key.to_string())).unwrap();
        assert_eq!(headers.get(0).key, "a-key");
        assert_eq!(headers.get(0).value.unwrap(), "a-value".as_bytes());
        assert_eq!(headers.get(1).key, "b-key");
        assert_eq!(headers.get(1).value.unwrap(), "b-value".as_bytes());
    }
}
