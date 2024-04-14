use bytes::Bytes;
use rdkafka::message::{Header, OwnedHeaders};
use vector_lib::lookup::OwnedTargetPath;
use vrl::path::{OwnedSegment, PathPrefix};

use crate::{
    internal_events::KafkaHeaderExtractionError,
    sinks::{
        kafka::service::{KafkaRequest, KafkaRequestMetadata},
        prelude::*,
    },
};

pub struct KafkaRequestBuilder {
    pub key_field: Option<OwnedTargetPath>,
    pub headers_key: Option<OwnedTargetPath>,
    pub encoder: (Transformer, Encoder<()>),
}

impl RequestBuilder<(String, Event)> for KafkaRequestBuilder {
    type Metadata = KafkaRequestMetadata;
    type Events = Event;
    type Encoder = (Transformer, Encoder<()>);
    type Payload = Bytes;
    type Request = KafkaRequest;
    type Error = std::io::Error;

    fn compression(&self) -> Compression {
        Compression::None
    }

    fn encoder(&self) -> &Self::Encoder {
        &self.encoder
    }

    fn split_input(
        &self,
        input: (String, Event),
    ) -> (Self::Metadata, RequestMetadataBuilder, Self::Events) {
        let (topic, mut event) = input;
        let builder = RequestMetadataBuilder::from_event(&event);

        let metadata = KafkaRequestMetadata {
            finalizers: event.take_finalizers(),
            key: get_key(&event, self.key_field.as_ref()),
            timestamp_millis: get_timestamp_millis(&event),
            headers: get_headers(&event, self.headers_key.as_ref()),
            topic,
        };

        (metadata, builder, event)
    }

    fn build_request(
        &self,
        metadata: Self::Metadata,
        request_metadata: RequestMetadata,
        payload: EncodeResult<Self::Payload>,
    ) -> Self::Request {
        KafkaRequest {
            body: payload.into_payload(),
            metadata,
            request_metadata,
        }
    }
}

fn get_key(event: &Event, key_field: Option<&OwnedTargetPath>) -> Option<Bytes> {
    key_field.and_then(|key_field| match event {
        Event::Log(log) => log.get(key_field).map(|value| value.coerce_to_bytes()),
        Event::Metric(metric) => metric_get(metric, key_field).map(|value| value.to_owned().into()),
        _ => None,
    })
}

// A version of this logic should be moved into "Metric" as "get" analogous to
// "LogEvent" when metrics can be interpreted as "Value"s.
pub fn metric_get<'a>(metric: &'a vector_lib::event::metric::Metric, key: &OwnedTargetPath) -> Option<&'a str> {
    match key.prefix {
        PathPrefix::Event =>
            match key.path.segments.get(0) {
                Some(OwnedSegment::Field(first_field)) =>
                    match first_field.as_ref() {
                        "name" => Some(metric.name()),
                        "tags" => match key.path.segments.len() {
                            2 => match key.path.segments.get(1) {
                                Some(OwnedSegment::Field(second_field)) => metric.tags().as_ref().and_then(|tags| tags.get(second_field.as_ref())),
                                _ => None,
                            } 
                            _ => None,
                        }
                        _ => metric.tags().as_ref().and_then(|tags| tags.get(key.to_string().as_str())),
                    }
                _ => None,
            }
        _ => None,
    }
}

fn get_timestamp_millis(event: &Event) -> Option<i64> {
    match &event {
        Event::Log(log) => log.get_timestamp().and_then(|v| v.as_timestamp()).copied(),
        Event::Metric(metric) => metric.timestamp(),
        _ => None,
    }
    .map(|ts| ts.timestamp_millis())
}

fn get_headers(event: &Event, headers_key: Option<&OwnedTargetPath>) -> Option<OwnedHeaders> {
    headers_key.and_then(|headers_key| {
        if let Event::Log(log) = event {
            if let Some(headers) = log.get(headers_key) {
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
    use bytes::Bytes;
    use rdkafka::message::Headers;

    use chrono::{offset::TimeZone, DateTime, Timelike, Utc};
    use similar_asserts::assert_eq;

    use super::*;
    use crate::event::{LogEvent, Metric, MetricKind, MetricTags, MetricValue, ObjectMap};

    #[test]
    fn kafka_get_headers() {
        let headers_key = OwnedTargetPath::try_from("headers".to_string()).unwrap();
        let mut header_values = ObjectMap::new();
        header_values.insert("a-key".into(), Value::Bytes(Bytes::from("a-value")));
        header_values.insert("b-key".into(), Value::Bytes(Bytes::from("b-value")));

        let mut event = Event::Log(LogEvent::from("hello"));
        event.as_mut_log().insert(&headers_key, header_values);

        let headers = get_headers(&event, Some(&headers_key)).unwrap();
        assert_eq!(headers.get(0).key, "a-key");
        assert_eq!(headers.get(0).value.unwrap(), "a-value".as_bytes());
        assert_eq!(headers.get(1).key, "b-key");
        assert_eq!(headers.get(1).value.unwrap(), "b-value".as_bytes());
    }

    fn ts() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2018, 11, 14, 8, 9, 10)
            .single()
            .and_then(|t| t.with_nanosecond(11))
            .expect("invalid timestamp")
    }

    fn tags() -> MetricTags {
        metric_tags!(
            "normal_tag" => "value1",
            ".workaround" => "value2",
        )
    }

    fn metric() -> Metric {
        Metric::new(
            "test_name",
            MetricKind::Incremental,
            MetricValue::Counter { value: 2.0 },
        )
        .with_namespace(Some("test_namespace"))
        .with_tags(Some(tags()))
        .with_timestamp(Some(ts()))
    }

    #[test]
    fn kafka_get_key_from_metric_with_workaround() {
        // Confirm direct reference to dot-prefixed tag names does not break
        let event = Event::Metric(metric());
        let key_field = OwnedTargetPath::try_from(".workaround".to_string()).unwrap();
        let key_value = get_key(&event, Some(&key_field));

        assert_eq!(key_value.unwrap().as_ref(), "value2".as_bytes());
    }

    #[test]
    fn kafka_get_key_from_metric_from_name() {
        let event = Event::Metric(metric());
        let key_field = OwnedTargetPath::try_from(".name".to_string()).unwrap();
        let key_value = get_key(&event, Some(&key_field));

        assert_eq!(key_value.unwrap().as_ref(), "test_name".as_bytes());
    }

    #[test]
    fn kafka_get_key_from_metric_from_normal_tag() {
        let event = Event::Metric(metric());
        let key_field = OwnedTargetPath::try_from(".tags.normal_tag".to_string()).unwrap();
        let key_value = get_key(&event, Some(&key_field));

        assert_eq!(key_value.unwrap().as_ref(), "value1".as_bytes());
    }

    #[test]
    fn kafka_get_key_from_metric_from_missing_tag() {
        let event = Event::Metric(metric());
        let key_field = OwnedTargetPath::try_from(".tags.missing_tag".to_string()).unwrap();
        let key_value = get_key(&event, Some(&key_field));

        assert_eq!(key_value, None);
    }

    #[test]
    fn kafka_get_key_from_metric_from_workaround_tag() {
        // Also test that explicitly referencing a workaround tag works as expected
        let event = Event::Metric(metric());
        let key_field = OwnedTargetPath::try_from(".tags.\".workaround\"".to_string()).unwrap();
        let key_value = get_key(&event, Some(&key_field));

        assert_eq!(key_value.unwrap().as_ref(), "value2".as_bytes());
    }
}
