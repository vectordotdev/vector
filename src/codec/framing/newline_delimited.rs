use crate::codec::{BoxedFramer, BoxedFramingError, FramingConfig};
use bytes::{Bytes, BytesMut};
use serde::{Deserialize, Serialize};
use tokio_util::codec::{Decoder, LinesCodec};

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct NewlineDelimitedDecoderConfig {
    max_length: Option<usize>,
}

impl NewlineDelimitedDecoderConfig {
    pub fn new(max_length: Option<usize>) -> Self {
        Self { max_length }
    }
}

#[typetag::serde(name = "newline_delimited")]
impl FramingConfig for NewlineDelimitedDecoderConfig {
    fn build(&self) -> BoxedFramer {
        Box::new(match self.max_length {
            Some(max_length) => NewlineDelimitedCodec::new_with_max_length(max_length),
            None => NewlineDelimitedCodec::new(),
        })
    }
}

#[derive(Debug, Clone)]
pub struct NewlineDelimitedCodec(LinesCodec);

impl NewlineDelimitedCodec {
    pub fn new() -> Self {
        Self(LinesCodec::new())
    }

    pub fn new_with_max_length(max_length: usize) -> Self {
        Self(LinesCodec::new_with_max_length(max_length))
    }
}

impl Default for NewlineDelimitedCodec {
    fn default() -> Self {
        Self::new()
    }
}

impl Decoder for NewlineDelimitedCodec {
    type Item = Bytes;
    type Error = BoxedFramingError;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        self.0
            .decode(src)
            .map(|item| item.map(Into::into))
            .map_err(Into::into)
    }

    fn decode_eof(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        self.0
            .decode_eof(src)
            .map(|item| item.map(Into::into))
            .map_err(Into::into)
    }
}
