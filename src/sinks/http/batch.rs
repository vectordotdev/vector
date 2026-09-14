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
            vector_lib::codecs::encoding::Serializer::NativeJson(_) => {
                let mut encoder = self.encoder.clone();
                let mut encoded = BytesMut::new();

                match encoder.encode(item.clone(), &mut encoded) {
                    Ok(()) => encoded.len(),
                    // The request builder will report the encoding error later. Keep batching
                    // infallible here because `ItemBatchSize` cannot propagate it.
                    Err(_) => item.size_of(),
                }
            }
            _ => item.size_of(),
        }
    }
}

#[cfg(test)]
mod tests {
    use vector_lib::{
        codecs::{NativeJsonSerializerConfig, NewlineDelimitedEncoderConfig},
        event::{LogEvent, Value},
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
        let mut encoded = BytesMut::new();
        encoder
            .encode(event, &mut encoded)
            .expect("native JSON event should encode");

        assert_eq!(batch_size, encoded.len());
        assert!(batch_size >= ordinary_json_size + 4096);
    }
}
