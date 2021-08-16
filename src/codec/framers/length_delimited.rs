// TODO.
#![allow(missing_docs)]

use crate::codec::{BoxedFramer, BoxedFramingError, FramingConfig};
use bytes::{Bytes, BytesMut};
use serde::{Deserialize, Serialize};
use tokio_util::codec::Decoder;

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct LengthDelimitedDecoderConfig;

#[typetag::serde(name = "length_delimited")]
impl FramingConfig for LengthDelimitedDecoderConfig {
    fn build(&self) -> BoxedFramer {
        Box::new(LengthDelimitedCodec::new())
    }
}

#[derive(Debug)]
pub struct LengthDelimitedCodec(tokio_util::codec::LengthDelimitedCodec);

impl LengthDelimitedCodec {
    pub fn new() -> Self {
        Self(tokio_util::codec::LengthDelimitedCodec::new())
    }
}

impl Default for LengthDelimitedCodec {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for LengthDelimitedCodec {
    fn clone(&self) -> Self {
        // This is an awful implementation for `Clone` since it resets the
        // internal state. However, it works for our use case because we
        // generally only clone a codec that has not been mutated yet.
        //
        // Ideally, `tokio_util::codec::LengthDelimitedCodec` should implement
        // `Clone` and it doesn't look like it was a deliberate decision to
        // leave out the implementation. All of its internal fields implement
        // `Clone`, so adding an implementation for `Clone` could be contributed
        // to the upstream repo easily by adding it to the `derive` macro.
        Self::new()
    }
}

impl Decoder for LengthDelimitedCodec {
    type Item = Bytes;
    type Error = BoxedFramingError;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        self.0
            .decode(src)
            .map(|bytes| bytes.map(Into::into))
            .map_err(Into::into)
    }

    fn decode_eof(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        self.0
            .decode(src)
            .map(|bytes| bytes.map(Into::into))
            .map_err(Into::into)
    }
}
