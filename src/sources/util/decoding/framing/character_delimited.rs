use crate::sources::util::decoding::{BoxedFramer, BytesDecoder, FramingConfig};
use codec::CharacterDelimitedCodec;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct CharacterDelimitedDecoderConfig {
    delimiter: char,
    max_length: Option<usize>,
}

#[typetag::serde(name = "character_delimited")]
impl FramingConfig for CharacterDelimitedDecoderConfig {
    fn build(&self) -> BoxedFramer {
        Box::new(BytesDecoder::new(match self.max_length {
            Some(max_length) => {
                CharacterDelimitedCodec::new_with_max_length(self.delimiter as u8, max_length)
            }
            None => CharacterDelimitedCodec::new(self.delimiter as u8),
        }))
    }
}
