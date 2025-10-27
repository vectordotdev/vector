//! Encoding for the `Pulsar` sink.
use std::io;

use bytes::BytesMut;
use tokio_util::codec::Encoder as _;
use vector_lib::{
    EstimatedJsonEncodedSizeOf, config::telemetry, request_metadata::GroupedCountByteSize,
};

use crate::{
    event::Event,
    sinks::util::encoding::{Encoder, write_all},
};

#[derive(Clone, Debug)]
pub(super) struct PulsarEncoder {
    pub(super) encoder: crate::codecs::Encoder<()>,
    pub(super) transformer: crate::codecs::Transformer,
}

impl Encoder<Event> for PulsarEncoder {
    fn encode_input(
        &self,
        mut input: Event,
        writer: &mut dyn io::Write,
    ) -> io::Result<(usize, GroupedCountByteSize)> {
        let mut body = BytesMut::new();
        self.transformer.transform(&mut input);

        let mut byte_size = telemetry().create_request_count_byte_size();
        byte_size.add_event(&input, input.estimated_json_encoded_size_of());

        let mut encoder = self.encoder.clone();
        encoder
            .encode(input, &mut body)
            .map_err(|_| io::Error::other("unable to encode"))?;

        let body = body.freeze();
        write_all(writer, 1, body.as_ref())?;

        Ok((body.len(), byte_size))
    }
}
