use super::{BoxedFramer, BoxedParser, BytesDecoder, Decoder};
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

impl FramingConfig {
    pub fn build(&self) -> BoxedFramer {
        use FramingConfig::*;

        match self {
            NewlineDelimited => Box::new(BytesDecoder::new(LinesCodec::new())),
            CharacterDelimited {
                delimiter,
                max_length,
            } => Box::new(BytesDecoder::new(match max_length {
                Some(max_length) => {
                    BytesDelimitedCodec::new_with_max_length(*delimiter as u8, *max_length)
                }
                None => BytesDelimitedCodec::new(*delimiter as u8),
            })),
            OctetCounting { max_length } => Box::new(BytesDecoder::new(match max_length {
                Some(max_length) => super::OctetCountingDecoder::new_with_max_length(*max_length),
                None => super::OctetCountingDecoder::new(),
            })),
        }
    }
}

#[derive(Debug, Copy, Clone, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ParserConfig {
    Bytes,
    #[cfg(feature = "sources-syslog")]
    Syslog,
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

impl DecodingConfig {
    pub fn build(&self) -> Decoder {
        let framer: BoxedFramer = match self.framing {
            Some(framing) => framing.build(),
            None => Box::new(super::BytesDecoder::new(BytesDelimitedCodec::new(b'\n'))),
        };

        let parser: BoxedParser = match self.decoding {
            Some(ParserConfig::Bytes) | None => Box::new(super::BytesParser),
            #[cfg(feature = "sources-syslog")]
            Some(ParserConfig::Syslog) => Box::new(super::SyslogParser),
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
