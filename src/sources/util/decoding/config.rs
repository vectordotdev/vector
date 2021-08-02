use super::{BytesDecoder, Decoder, Parser};
use bytes::Bytes;
use codec::BytesDelimitedCodec;
use serde::{Deserialize, Serialize};

#[derive(Debug, Copy, Clone, Deserialize, Serialize)]
pub enum FramingConfig {
    BytesDelimited {
        delimiter: u8,
        max_length: Option<usize>,
    },
}

impl From<FramingConfig>
    for Box<
        dyn tokio_util::codec::Decoder<Item = Bytes, Error = super::Error> + Send + Sync + 'static,
    >
{
    fn from(config: FramingConfig) -> Self {
        match config {
            FramingConfig::BytesDelimited {
                delimiter,
                max_length,
            } => {
                let decoder = BytesDecoder::new(match max_length {
                    Some(max_length) => {
                        BytesDelimitedCodec::new_with_max_length(delimiter, max_length)
                    }
                    None => BytesDelimitedCodec::new(delimiter),
                });
                Box::new(decoder)
            }
        }
    }
}

#[derive(Debug, Copy, Clone, Deserialize, Serialize)]
pub enum ParserConfig {
    Bytes,
}

#[derive(Debug, Copy, Clone, Default, Deserialize, Serialize)]
pub struct DecodingConfig {
    framing: Option<FramingConfig>,
    parser: Option<ParserConfig>,
}

impl DecodingConfig {
    pub fn new(framing: Option<FramingConfig>, parser: Option<ParserConfig>) -> Self {
        Self { framing, parser }
    }
}

impl From<DecodingConfig> for Decoder {
    fn from(config: DecodingConfig) -> Self {
        let framer: Box<
            dyn tokio_util::codec::Decoder<Item = Bytes, Error = super::Error>
                + Send
                + Sync
                + 'static,
        > = match config.framing {
            Some(framing) => framing.into(),
            None => Box::new(super::BytesDecoder::new(BytesDelimitedCodec::new(b'\n'))),
        };

        let parser: Box<dyn Parser + Send + Sync + 'static> = match config.parser {
            Some(ParserConfig::Bytes) | None => Box::new(super::BytesParser),
        };

        Decoder::new(framer, parser)
    }
}
