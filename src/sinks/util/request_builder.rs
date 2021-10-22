use std::io;

use super::{encoding::Encoder, Compression, Compressor};

/// Generalized interface for defining how a batch of events will be turned into an request.
pub trait RequestBuilder<Input> {
    type Metadata;
    type Events;
    type Encoder: Encoder<Self::Events>;
    type Payload: From<Vec<u8>>;
    type Request;
    type Error: From<io::Error>;

    fn compression(&self) -> Compression;

    fn encoder(&self) -> &Self::Encoder;

    /// Splits apart the input into the metadata and event portions.
    ///
    /// The metadata should be any information that needs to be passed back to `build_request`
    /// as-is, such as event finalizers, while the events are the actual events to process.
    fn split_input(&self, input: Input) -> (Self::Metadata, Self::Events);

    fn encode_events(&self, events: Self::Events) -> Result<Self::Payload, Self::Error> {
        let mut compressor = Compressor::from(self.compression());
        let _ = self.encoder().encode_input(events, &mut compressor)?;

        let payload = compressor.into_inner().into();
        Ok(payload)
    }

    /// Builds a request for the given metadata and payload.
    fn build_request(&self, metadata: Self::Metadata, payload: Self::Payload) -> Self::Request;
}
