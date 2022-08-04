use bytes::BytesMut;
use serde::{Deserialize, Serialize};
use tokio_util::codec::{Encoder, LengthDelimitedCodec};

use super::BoxedFramingError;

/// Config used to build a `LengthDelimitedEncoder`.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LengthDelimitedEncoderConfig;

impl LengthDelimitedEncoderConfig {
    /// Creates a `LengthDelimitedEncoderConfig`.
    pub const fn new() -> Self {
        Self
    }

    /// Build the `LengthDelimitedEncoder` from this configuration.
    pub fn build(&self) -> LengthDelimitedEncoder {
        LengthDelimitedEncoder::new()
    }
}

/// An encoder for handling bytes that are delimited by a length header.
#[derive(Debug)]
pub struct LengthDelimitedEncoder(LengthDelimitedCodec);

impl LengthDelimitedEncoder {
    /// Creates a `LengthDelimitedEncoder`.
    pub fn new() -> Self {
        Self(LengthDelimitedCodec::new())
    }
}

impl Default for LengthDelimitedEncoder {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for LengthDelimitedEncoder {
    fn clone(&self) -> Self {
        // This has been fixed with https://github.com/tokio-rs/tokio/pull/4089,
        // however we are blocked on upgrading to a new release of `tokio-util`
        // that includes the `Clone` implementation:
        // https://github.com/vectordotdev/vector/issues/11257.
        //
        // This is an awful implementation for `Clone` since it resets the
        // internal state. However, it works for our use case because we
        // generally only clone a codec that has not been mutated yet.
        //
        // Ideally, `tokio_util::codec::LengthDelimitedCodec` should implement
        // `Clone` and it doesn't look like it was a deliberate decision to
        // leave out the implementation. All of its internal fields implement
        // `Clone`, so adding an implementation for `Clone` could be contributed
        // to the upstream repo easily by adding it to the `derive` macro.
        Self::new()
    }
}

impl Encoder<()> for LengthDelimitedEncoder {
    type Error = BoxedFramingError;

    fn encode(&mut self, _: (), buffer: &mut BytesMut) -> Result<(), BoxedFramingError> {
        let bytes = buffer.split().freeze();
        self.0.encode(bytes, buffer)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode() {
        let mut codec = LengthDelimitedEncoder::new();

        let mut buffer = BytesMut::from("abc");
        codec.encode((), &mut buffer).unwrap();

        assert_eq!(&buffer[..], b"\0\0\0\x03abc");
    }
}
