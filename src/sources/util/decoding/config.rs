use super::{BoxedFramer, BoxedParser, BytesParserConfig, Decoder, NewlineDelimitedDecoderConfig};
use core::fmt::Debug;
use dyn_clone::DynClone;
use serde::{Deserialize, Serialize};

#[typetag::serde(tag = "method")]
pub trait FramingConfig: Debug + DynClone + Send + Sync {
    fn build(&self) -> BoxedFramer;
}

dyn_clone::clone_trait_object!(FramingConfig);

#[typetag::serde(tag = "codec")]
pub trait ParserConfig: Debug + DynClone + Send + Sync {
    fn build(&self) -> BoxedParser;
}

dyn_clone::clone_trait_object!(ParserConfig);

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct DecodingConfig {
    framing: Option<Box<dyn FramingConfig>>,
    decoding: Option<Box<dyn ParserConfig>>,
}

impl DecodingConfig {
    pub fn new(
        framing: Option<Box<dyn FramingConfig>>,
        decoding: Option<Box<dyn ParserConfig>>,
    ) -> Self {
        Self { framing, decoding }
    }
}

impl DecodingConfig {
    pub fn build(&self) -> Decoder {
        let framer: BoxedFramer = match &self.framing {
            Some(config) => config.build(),
            None => NewlineDelimitedDecoderConfig::new(None).build(),
        };

        let parser: BoxedParser = match &self.decoding {
            Some(config) => config.build(),
            None => BytesParserConfig::new().build(),
        };

        Decoder::new(framer, parser)
    }
}
