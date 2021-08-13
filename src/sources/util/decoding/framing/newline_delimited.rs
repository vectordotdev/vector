use crate::sources::util::decoding::{BoxedFramer, BytesDecoder, FramingConfig};
use serde::{Deserialize, Serialize};
use tokio_util::codec::LinesCodec;

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
        Box::new(BytesDecoder::new(match self.max_length {
            Some(max_length) => LinesCodec::new_with_max_length(max_length),
            None => LinesCodec::new(),
        }))
    }
}
