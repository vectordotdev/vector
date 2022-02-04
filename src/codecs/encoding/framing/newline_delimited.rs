use bytes::BytesMut;
use serde::{Deserialize, Serialize};
use tokio_util::codec::Encoder;

use super::{BoxedFramer, BoxedFramingError, CharacterDelimitedEncoder, FramingConfig};

/// Config used to build a `NewlineDelimitedEncoder`.
#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq)]
pub struct NewlineDelimitedEncoderConfig;

impl NewlineDelimitedEncoderConfig {
    /// Creates a new `NewlineDelimitedEncoderConfig`.
    pub fn new() -> Self {
        Default::default()
    }
}

#[typetag::serde(name = "newline_delimited")]
impl FramingConfig for NewlineDelimitedEncoderConfig {
    fn build(&self) -> crate::Result<BoxedFramer> {
        Ok(Box::new(NewlineDelimitedEncoder::new()))
    }
}

/// A codec for handling bytes that are delimited by (a) newline(s).
#[derive(Debug, Clone)]
pub struct NewlineDelimitedEncoder(CharacterDelimitedEncoder);

impl NewlineDelimitedEncoder {
    /// Creates a new `NewlineDelimitedEncoder`.
    pub const fn new() -> Self {
        Self(CharacterDelimitedEncoder::new(b'\n'))
    }
}

impl Encoder<()> for NewlineDelimitedEncoder {
    type Error = BoxedFramingError;

    fn encode(&mut self, _: (), buffer: &mut BytesMut) -> Result<(), BoxedFramingError> {
        self.0.encode((), buffer)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_bytes() {
        let mut input = BytesMut::from("foo");
        let mut encoder = NewlineDelimitedEncoder::new();

        encoder.encode((), &mut input).unwrap();

        assert_eq!(input, "foo\n");
    }
}
