use bytes::BytesMut;
use tokio_util::codec::Encoder as _;
use vector_core::config::telemetry;

use crate::sinks::{prelude::*, util::EncodedLength};

use super::{RedisEvent, RedisKvEntry, RedisRequest};

fn encode_event(
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

// pub(super) struct RedisRequestBuilder {
//     encoder: RedisEncoder,
// }

// impl RequestBuilder<Vec<RedisEvent>> for RedisRequestBuilder {
//     type Metadata = RedisMetadata;
//     type Events = Vec<RedisEvent>;
//     type Encoder = RedisEncoder;
//     type Payload = Vec<RedisKvEntry>;
//     type Request = RedisRequest;
//     type Error = io::Error;

//     fn compression(&self) -> Compression {
//         Compression::None
//     }

//     fn encoder(&self) -> &Self::Encoder {
//         &self.encoder
//     }

//     fn encode_events(
//         &self,
//         events: Self::Events,
//     ) -> Result<EncodeResult<Self::Payload>, Self::Error> {
//         let mut compressor = Compressor::from(self.compression());
//         let is_compressed = compressor.is_compressed();
//         let entries = Vec::new();

//         let mut byte_size = telemetry().create_request_count_byte_size();

//         for event in self.events {
//             self.transformer.transform(&mut event);

//             byte_size.add_event(&event, event.estimated_json_encoded_size_of());

//             let mut bytes = BytesMut::new();

//             // Errors are handled by `Encoder`.
//             self.encoder
//                 .encode(input, &mut bytes)
//                 .map_err(|_| io::Error::new(io::ErrorKind::Other, "unable to encode"))?;

//             let body = bytes.freeze();
//             write_all(writer, 1, body.as_ref())?;
//         }

//         let result = if is_compressed {
//             let compressed_byte_size = payload.len();
//             EncodeResult::compressed(payload.into(), compressed_byte_size, json_size)
//         } else {
//             EncodeResult::uncompressed(payload.into(), json_size)
//         };

//         Ok(result)
//     }

//     fn split_input(
//         &self,
//         input: RedisEvent,
//     ) -> (Self::Metadata, RequestMetadataBuilder, Self::Events) {
//         let builder = RequestMetadataBuilder::from_event(&input.event);
//         let metadata = RedisMetadata { key: input.key };

//         (metadata, builder, input)
//     }

//     fn build_request(
//         &self,
//         metadata: Self::Metadata,
//         request_metadata: RequestMetadata,
//         payload: EncodeResult<Self::Payload>,
//     ) -> Self::Request {
//         let value = payload.into_payload();

//         RedisRequest {
//             request: RedisKvEntry {
//                 key: metadata.key,
//                 value,
//             },
//         }
//     }
// }
