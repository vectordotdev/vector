// TODO.
#![allow(missing_docs)]

use crate::codec::{BoxedFramer, BoxedFramingError, FramingConfig};
use bytes::{Bytes, BytesMut};
use serde::{Deserialize, Serialize};
use tokio_util::codec::Decoder;

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct BytesDecoderConfig;

#[typetag::serde(name = "bytes")]
impl FramingConfig for BytesDecoderConfig {
    fn build(&self) -> BoxedFramer {
        Box::new(BytesCodec::new())
    }
}

#[derive(Debug, Clone)]
pub struct BytesCodec(tokio_util::codec::BytesCodec);

impl BytesCodec {
    pub fn new() -> Self {
        Self(tokio_util::codec::BytesCodec::new())
    }
}

impl Default for BytesCodec {
    fn default() -> Self {
        Self::new()
    }
}

impl Decoder for BytesCodec {
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
