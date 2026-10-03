//! Batch settings for the `http` sink.

use bytes::BytesMut;
use tokio_util::codec::Encoder as _;
use vector_lib::{
    ByteSizeOf, EstimatedJsonEncodedSizeOf, codecs::encoding::Framer, event::Event,
    stream::batcher::limiter::ItemBatchSize,
};

use vector_lib::codecs::Encoder;

/// Uses the configured encoder to determine batch sizing.
#[derive(Default, Clone)]
pub(super) struct HttpBatchSizer {
    pub(super) encoder: Encoder<Framer>,
}

impl ItemBatchSize<Event> for HttpBatchSizer {
    fn size(&self, item: &Event) -> usize {
        match self.encoder.serializer() {
            vector_lib::codecs::encoding::Serializer::Json(_)
            | vector_lib::codecs::encoding::Serializer::NativeJson(_) => {
                item.estimated_json_encoded_size_of().get()
            }
            // The OTLP serializer produces a compact protobuf payload from a much larger
            // in-memory JSON-shaped `Event::Log`, so `size_of()` (used by the fallback arm
            // below) overstates the real wire size by an order of magnitude and causes
            // `max_bytes` to trigger far too early. Encode the item for real to get an
            // accurate size; cloning the serializer and the item are both cheap (Arc bumps),
            // and OTLP framing is a no-op, so this mirrors exactly what the real request
            // encoder does per-event.
            #[cfg(feature = "codecs-opentelemetry")]
            vector_lib::codecs::encoding::Serializer::Otlp(_) => {
                let mut serializer = self.encoder.serializer().clone();
                let mut buffer = BytesMut::new();
                match serializer.encode(item.clone(), &mut buffer) {
                    Ok(()) => buffer.len(),
                    Err(_) => item.size_of(),
                }
            }
            _ => item.size_of(),
        }
    }
}

#[cfg(all(test, feature = "codecs-opentelemetry"))]
mod tests {
    use super::*;
    use serde_json::json;
    use vector_lib::codecs::encoding::{BytesEncoder, OtlpSerializerConfig, Serializer};
    use vector_lib::event::LogEvent;

    fn otlp_metric_event() -> Event {
        let json = json!({
            "resourceMetrics": [{
                "resource": {
                    "attributes": [{
                        "key": "service.name",
                        "value": { "stringValue": "test-service" },
                    }],
                },
                "scopeMetrics": [{
                    "scope": { "name": "test-scope" },
                    "metrics": [{
                        "name": "test.metric",
                        "sum": {
                            "dataPoints": [{
                                "attributes": [{
                                    "key": "env",
                                    "value": { "stringValue": "prod" },
                                }],
                                "startTimeUnixNano": 0,
                                "timeUnixNano": 0,
                                "asDouble": 42.0,
                            }],
                            "aggregationTemporality": 1,
                            "isMonotonic": true,
                        },
                    }],
                }],
            }],
        });

        Event::Log(LogEvent::try_from(json).expect("valid OTLP JSON converts to a LogEvent"))
    }

    #[test]
    fn otlp_batch_size_matches_real_wire_bytes_and_beats_size_of() {
        let event = otlp_metric_event();

        let serializer = Serializer::Otlp(
            OtlpSerializerConfig::default()
                .build()
                .expect("serializer builds"),
        );
        let sizer = HttpBatchSizer {
            encoder: Encoder::<Framer>::new(BytesEncoder.into(), serializer.clone()),
        };

        let mut serializer = serializer;
        let mut wire_bytes = BytesMut::new();
        serializer
            .encode(event.clone(), &mut wire_bytes)
            .expect("event encodes to OTLP protobuf");

        assert_eq!(
            sizer.size(&event),
            wire_bytes.len(),
            "HttpBatchSizer must report the real OTLP wire size"
        );
        assert!(
            sizer.size(&event) < event.size_of(),
            "OTLP wire size ({}) should be far smaller than the in-memory size_of() ({}) \
             that the batcher previously (incorrectly) used for max_bytes accounting",
            sizer.size(&event),
            event.size_of(),
        );
    }
}
