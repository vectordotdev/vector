mod json;
mod string;

pub use json::JsonEncoderConfig;
pub use string::StringEncoderConfig;

use crate::record::Record;
use bytes::Bytes;

#[typetag::serde(tag = "type")]
pub trait EncoderConfig: core::fmt::Debug {
    fn build(&self) -> Box<dyn Encoder + Send>;
}

pub trait Encoder {
    fn encode(&self, record: Record) -> Bytes;
}

pub fn default_string_encoder() -> Box<dyn EncoderConfig> {
    Box::new(StringEncoderConfig {})
}
