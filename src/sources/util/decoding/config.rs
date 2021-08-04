use super::{BytesDecoder, Decoder, Parser};
use bytes::Bytes;
use codec::BytesDelimitedCodec;
use serde::{Deserialize, Serialize};
use tokio_util::codec::LinesCodec;

#[derive(Debug, Copy, Clone, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum FramingConfig {
    NewlineDelimited,
    CharacterDelimited {
        delimiter: char,
        max_length: Option<usize>,
    },
    OctetCounting {
        max_length: Option<usize>,
    },
}

impl From<FramingConfig>
    for Box<
        dyn tokio_util::codec::Decoder<Item = Bytes, Error = super::Error> + Send + Sync + 'static,
    >
{
    fn from(config: FramingConfig) -> Self {
        use FramingConfig::*;

        match config {
            NewlineDelimited => Box::new(BytesDecoder::new(LinesCodec::new())),
            CharacterDelimited {
                delimiter,
                max_length,
            } => Box::new(BytesDecoder::new(match max_length {
                Some(max_length) => {
                    BytesDelimitedCodec::new_with_max_length(delimiter as u8, max_length)
                }
                None => BytesDelimitedCodec::new(delimiter as u8),
            })),
            OctetCounting { max_length } => Box::new(BytesDecoder::new(match max_length {
                Some(max_length) => super::OctetCountingDecoder::new_with_max_length(max_length),
                None => super::OctetCountingDecoder::new(),
            })),
        }
    }
}

#[derive(Debug, Copy, Clone, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ParserConfig {
    Bytes,
}

#[derive(Debug, Copy, Clone, Default, Deserialize, Serialize)]
pub struct DecodingConfig {
    framing: Option<FramingConfig>,
    decoding: Option<ParserConfig>,
}

impl DecodingConfig {
    pub fn new(framing: Option<FramingConfig>, decoding: Option<ParserConfig>) -> Self {
        Self { framing, decoding }
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

        let parser: Box<dyn Parser + Send + Sync + 'static> = match config.decoding {
            Some(ParserConfig::Bytes) | None => Box::new(super::BytesParser),
        };

        Decoder::new(framer, parser)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use indoc::indoc;

    #[test]
    fn config_with_framing_and_decoding() {
        let config: DecodingConfig = toml::from_str(indoc! {r#"
            [framing]
            type = "character_delimited"
            delimiter = "\t"
            max_length = 1337

            [decoding]
            type = "bytes"
        "#})
        .unwrap();

        assert!(matches!(
            config.framing,
            Some(FramingConfig::CharacterDelimited {
                delimiter: '\t',
                max_length: Some(1337)
            })
        ));

        assert!(matches!(config.decoding, Some(ParserConfig::Bytes)));
    }
}
