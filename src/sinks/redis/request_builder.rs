use bytes::BytesMut;
use tokio_util::codec::Encoder as _;
use vector_lib::config::telemetry;

use crate::sinks::{prelude::*, util::EncodedLength};

use super::{RedisEvent, RedisKvEntry, RedisRequest};

pub(super) fn encode_event(
    mut event: Event,
    key: String,
    transformer: &Transformer,
    encoder: &mut Encoder<()>,
    byte_size: &mut GroupedCountByteSize,
) -> Option<RedisKvEntry> {
    transformer.transform(&mut event);
    byte_size.add_event(&event, event.estimated_json_encoded_size_of());

    let mut bytes = BytesMut::new();

    // Errors are handled by `Encoder`.
    encoder.encode(event, &mut bytes).ok()?;

    let value = bytes.freeze();

    let event = RedisKvEntry { key, value };
    Some(event)
}

fn encode_events(
    events: Vec<RedisEvent>,
    transformer: &Transformer,
    encoder: &mut Encoder<()>,
) -> EncodeResult<Vec<RedisKvEntry>> {
    let mut byte_size = telemetry().create_request_count_byte_size();
    let request = events
        .into_iter()
        .filter_map(|event| {
            encode_event(event.event, event.key, transformer, encoder, &mut byte_size)
        })
        .collect::<Vec<_>>();

    let uncompressed_byte_size = request.iter().map(|event| event.encoded_length()).sum();

    EncodeResult {
        payload: request,
        uncompressed_byte_size,
        transformed_json_size: byte_size,
        compressed_byte_size: None,
    }
}

/// Builds the request to be sent to Redis.
/// The `[RequestBuilder]` trait doesn't work since the encoded event is not just `Byte`s.
/// This function allows us to accept a list of `Event`s and return a list of key -> encoded
/// event objects.
pub(super) fn request_builder(
    mut events: Vec<RedisEvent>,
    transformer: &Transformer,
    encoder: &mut Encoder<()>,
) -> RedisRequest {
    let finalizers = events.take_finalizers();
    let builder = RequestMetadataBuilder::from_events(&events);
    let encoded = encode_events(events, transformer, encoder);
    let metadata = builder.build(&encoded);

    RedisRequest {
        request: encoded.into_payload(),
        finalizers,
        metadata,
    }
}
