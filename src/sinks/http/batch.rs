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
            vector_lib::codecs::encoding::Serializer::Json(_) => {
                item.estimated_json_encoded_size_of().get()
            }
            vector_lib::codecs::encoding::Serializer::NativeJson(serializer) => {
                let mut serializer = serializer.clone();
                let mut framer = self.encoder.framer().clone();
                let mut buffer = BytesMut::new();

                // Probe without the encoder wrapper's error and drop telemetry: request-time
                // field filtering may still make an event encodable.
                if serializer.encode(item.clone(), &mut buffer).is_ok()
                    && framer.encode((), &mut buffer).is_ok()
                {
                    buffer.len()
                } else {
                    // Keep sizing infallible. Request encoding reports any remaining errors.
                    item.size_of()
                }
            }
            _ => item.size_of(),
        }
    }
}

#[cfg(test)]
mod tests {
    use vector_lib::{
        codecs::{
            LengthDelimitedEncoderConfig, NativeJsonSerializerConfig, NewlineDelimitedEncoderConfig,
        },
        event::{LogEvent, Value},
        metrics::{self, Controller},
    };

    use super::*;

    #[test]
    fn native_json_uses_encoded_size() {
        let mut encoder = Encoder::<Framer>::new(
            NewlineDelimitedEncoderConfig.build().into(),
            NativeJsonSerializerConfig.build().into(),
        );
        let sizer = HttpBatchSizer {
            encoder: encoder.clone(),
        };
        let mut event = Event::Log(LogEvent::from("small"));
        *event.metadata_mut().value_mut() = Value::Bytes(vec![b'x'; 4096].into());

        let batch_size = sizer.size(&event);
        let ordinary_json_size = event.estimated_json_encoded_size_of().get();
        let mut buffer = BytesMut::new();
        encoder
            .encode(event, &mut buffer)
            .expect("native JSON event should encode");

        assert_eq!(batch_size, buffer.len());
        assert!(batch_size >= ordinary_json_size + 4096);
    }

    #[test]
    fn native_json_framing_error_uses_fallback_without_telemetry() {
        metrics::init_test();
        let controller = Controller::get().unwrap();
        controller.reset();

        let mut framing = LengthDelimitedEncoderConfig::default();
        framing.length_delimited.max_frame_length = 1;
        let sizer = HttpBatchSizer {
            encoder: Encoder::<Framer>::new(
                framing.build().into(),
                NativeJsonSerializerConfig.build().into(),
            ),
        };
        let event = Event::Log(LogEvent::from("larger than the frame limit"));

        assert_eq!(sizer.size(&event), event.size_of());
        let metrics = controller.capture_metrics();
        for name in ["component_errors_total", "component_discarded_events_total"] {
            assert!(
                !metrics.iter().any(|metric| metric.name() == name),
                "batch sizing must not report {name} for a framing failure"
            );
        }
    }
}
