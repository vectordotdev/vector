// use bytes::BytesMut;

// use crate::sinks::prelude::*;

// use super::RedisEvent;

// pub(super) struct RedisEncoder {
//     encoder: crate::codecs::Encoder<()>,
//     transformer: crate::codecs::Transformer,
// }

// impl encoding::Encoder<Vec<RedisEvent>> for RedisEncoder {
//     fn encode_input(
//         &self,
//         input: Vec<RedisEvent>,
//         writer: &mut dyn std::io::Write,
//     ) -> std::io::Result<(usize, GroupedCountByteSize)> {
//         // self.transformer.transform(&mut input);

//         // let mut byte_size = telemetry().create_request_count_byte_size();
//         // byte_size.add_event(&input, input.estimated_json_encoded_size_of());

//         // let mut bytes = BytesMut::new();

//         // // Errors are handled by `Encoder`.
//         // self.encoder
//         //     .encode(input, &mut bytes)
//         //     .map_err(|_| io::Error::new(io::ErrorKind::Other, "unable to encode"))?;

//         // let body = bytes.freeze();
//         // write_all(writer, 1, body.as_ref())?;

//         // Ok((body.len(), byte_size))
//     }
// }

// // fn encode_event(
// //     mut event: Event,
// //     key: &Template,
// //     transformer: &Transformer,
// //     encoder: &mut Encoder<()>,
// // ) -> Option<super::util::EncodedEvent<RedisKvEntry>> {
// //     let key = key
// //         .render_string(&event)
// //         .map_err(|error| {
// //             emit!(TemplateRenderingError {
// //                 error,
// //                 field: Some("key"),
// //                 drop_event: true,
// //             });
// //         })
// //         .ok()?;

// //     let event_byte_size = event.estimated_json_encoded_size_of();

// //     transformer.transform(&mut event);

// //     let mut bytes = BytesMut::new();

// //     // Errors are handled by `Encoder`.
// //     encoder.encode(event, &mut bytes).ok()?;

// //     let byte_size = bytes.len();
// //     let value = bytes.freeze();

// //     let event = EncodedEvent::new(RedisKvEntry { key, value }, byte_size, event_byte_size);
// //     Some(event)
// // }
