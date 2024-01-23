use std::io;

use bytes::{Buf, Bytes, BytesMut};
use derivative::Derivative;
use tokio_util::codec::{LinesCodec, LinesCodecError};
use tracing::trace;
use vector_config::configurable_component;

use super::BoxedFramingError;

/// Config used to build a `OctetCountingDecoder`.
#[configurable_component]
#[derive(Debug, Clone, Default)]
pub struct OctetCountingDecoderConfig {
    #[serde(default, skip_serializing_if = "vector_core::serde::is_default")]
    /// Options for the octet counting decoder.
    pub octet_counting: OctetCountingDecoderOptions,
}

impl OctetCountingDecoderConfig {
    /// Build the `OctetCountingDecoder` from this configuration.
    pub fn build(&self) -> OctetCountingDecoder {
        if let Some(max_length) = self.octet_counting.max_length {
            OctetCountingDecoder::new_with_max_length(max_length)
        } else {
            OctetCountingDecoder::new()
        }
    }
}

/// Options for building a `OctetCountingDecoder`.
#[configurable_component]
#[derive(Clone, Debug, Derivative, PartialEq, Eq)]
#[derivative(Default)]
pub struct OctetCountingDecoderOptions {
    /// The maximum length of the byte buffer.
    #[serde(skip_serializing_if = "vector_core::serde::is_default")]
    pub max_length: Option<usize>,
}

/// Codec using the `Octet Counting` format as specified in
/// <https://tools.ietf.org/html/rfc6587#section-3.4.1>.
#[derive(Clone, Debug)]
pub struct OctetCountingDecoder {
    other: LinesCodec,
    octet_decoding: Option<State>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    NotDiscarding,
    Discarding(usize),
    DiscardingToEol,
}

impl OctetCountingDecoder {
    /// Creates a new `OctetCountingDecoder`.
    pub fn new() -> Self {
        Self {
            other: LinesCodec::new(),
            octet_decoding: None,
        }
    }

    /// Creates a `OctetCountingDecoder` with a maximum frame length limit.
    pub fn new_with_max_length(max_length: usize) -> Self {
        Self {
            other: LinesCodec::new_with_max_length(max_length),
            octet_decoding: None,
        }
    }

    /// Decode a frame.
    fn octet_decode(
        &mut self,
        state: State,
        src: &mut BytesMut,
    ) -> Result<Option<Bytes>, LinesCodecError> {
        // Encoding scheme:
        //
        // len ' ' data
        // |    |  | len number of bytes that contain syslog message
        // |    |
        // |    | Separating whitespace
        // |
        // | ASCII decimal number of unknown length

        let space_pos = src.iter().position(|&b| b == b' ');

        // If we are discarding, discard to the next newline.
        let newline_pos = src.iter().position(|&b| b == b'\n');

        match (state, newline_pos, space_pos) {
            (State::Discarding(chars), _, _) if src.len() >= chars => {
                // We have a certain number of chars to discard.
                //
                // There are enough chars in this frame to discard
                src.advance(chars);
                self.octet_decoding = None;
                Err(LinesCodecError::Io(io::Error::new(
                    io::ErrorKind::Other,
                    "Frame length limit exceeded",
                )))
            }

            (State::Discarding(chars), _, _) => {
                // We have a certain number of chars to discard.
                //
                // There aren't enough in this frame so we need to discard the
                // entire frame and adjust the amount to discard accordingly.
                self.octet_decoding = Some(State::Discarding(src.len() - chars));
                src.advance(src.len());
                Ok(None)
            }

            (State::DiscardingToEol, Some(offset), _) => {
                // When discarding we keep discarding to the next newline.
                src.advance(offset + 1);
                self.octet_decoding = None;
                Err(LinesCodecError::Io(io::Error::new(
                    io::ErrorKind::Other,
                    "Frame length limit exceeded",
                )))
            }

            (State::DiscardingToEol, None, _) => {
                // There is no newline in this frame.
                //
                // Since we don't have a set number of chars we want to discard,
                // we need to discard to the next newline. Advance as far as we
                // can to discard the entire frame.
                src.advance(src.len());
                Ok(None)
            }

            (State::NotDiscarding, _, Some(space_pos)) if space_pos < self.other.max_length() => {
                // Everything looks good.
                //
                // We aren't discarding, we have a space that is not beyond our
                // maximum length. Attempt to parse the bytes as a number which
                // will hopefully give us a sensible length for our message.
                let len: usize = match std::str::from_utf8(&src[..space_pos])
                    .map_err(|_| ())
                    .and_then(|num| num.parse().map_err(|_| ()))
                {
                    Ok(len) => len,
                    Err(_) => {
                        // It was not a sensible number.
                        //
                        // Advance the buffer past the erroneous bytes to
                        // prevent us getting stuck in an infinite loop.
                        src.advance(space_pos + 1);
                        self.octet_decoding = None;
                        return Err(LinesCodecError::Io(io::Error::new(
                            io::ErrorKind::InvalidData,
                            "Unable to decode message len as number",
                        )));
                    }
                };

                let from = space_pos + 1;
                let to = from + len;

                if len > self.other.max_length() {
                    // The length is greater than we want.
                    //
                    // We need to discard the entire message.
                    self.octet_decoding = Some(State::Discarding(len));
                    src.advance(space_pos + 1);

                    Ok(None)
                } else if let Some(msg) = src.get(from..to) {
                    let bytes = match std::str::from_utf8(msg) {
                        Ok(_) => Bytes::copy_from_slice(msg),
                        Err(_) => {
                            // The data was not valid UTF8 :-(.
                            //
                            // Advance the buffer past the erroneous bytes to
                            // prevent us getting stuck in an infinite loop.
                            src.advance(to);
                            self.octet_decoding = None;
                            return Err(LinesCodecError::Io(io::Error::new(
                                io::ErrorKind::InvalidData,
                                "Unable to decode message as UTF8",
                            )));
                        }
                    };

                    // We have managed to read the entire message as valid UTF8!
                    src.advance(to);
                    self.octet_decoding = None;
                    Ok(Some(bytes))
                } else {
                    // We have an acceptable number of bytes in this message,
                    // but not all the data was in the frame.
                    //
                    // Return `None` to indicate we want more data before we do
                    // anything else.
                    Ok(None)
                }
            }

            (State::NotDiscarding, Some(newline_pos), _) => {
                // Beyond maximum length, advance to the newline.
                src.advance(newline_pos + 1);
                Err(LinesCodecError::Io(io::Error::new(
                    io::ErrorKind::Other,
                    "Frame length limit exceeded",
                )))
            }

            (State::NotDiscarding, None, _) if src.len() < self.other.max_length() => {
                // We aren't discarding, but there is no useful character to
                // tell us what to do next.
                //
                // We are still not beyond the max length, so just return `None`
                // to indicate we need to wait for more data.
                Ok(None)
            }

            (State::NotDiscarding, None, _) => {
                // There is no newline in this frame and we have more data than
                // we want to handle.
                //
                // Advance as far as we can to discard the entire frame.
                self.octet_decoding = Some(State::DiscardingToEol);
                src.advance(src.len());
                Ok(None)
            }
        }
    }

    /// `None` if this is not octet counting encoded.
    fn checked_decode(
        &mut self,
        src: &mut BytesMut,
    ) -> Option<Result<Option<Bytes>, LinesCodecError>> {
        if let Some(&first_byte) = src.first() {
            if (49..=57).contains(&first_byte) {
                // First character is non zero number so we can assume that
                // octet count framing is used.
                trace!("Octet counting encoded event detected.");
                self.octet_decoding = Some(State::NotDiscarding);
            }
        }

        self.octet_decoding
            .map(|state| self.octet_decode(state, src))
    }
}

impl Default for OctetCountingDecoder {
    fn default() -> Self {
        Self::new()
    }
}

impl tokio_util::codec::Decoder for OctetCountingDecoder {
    type Item = Bytes;
    type Error = BoxedFramingError;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        if let Some(ret) = self.checked_decode(src) {
            ret
        } else {
            // Octet counting isn't used so fallback to newline codec.
            self.other
                .decode(src)
                .map(|line| line.map(|line| line.into()))
        }
        .map_err(Into::into)
    }

    fn decode_eof(&mut self, buf: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        if let Some(ret) = self.checked_decode(buf) {
            ret
        } else {
            // Octet counting isn't used so fallback to newline codec.
            self.other
                .decode_eof(buf)
                .map(|line| line.map(|line| line.into()))
        }
        .map_err(Into::into)
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::print_stdout)]

    use bytes::BufMut;
    use tokio_util::codec::Decoder;

    use super::*;

    #[test]
    fn non_octet_decode_works_with_multiple_frames() {
        let mut decoder = OctetCountingDecoder::new_with_max_length(128);
        let mut buffer = BytesMut::with_capacity(16);

        buffer.put(&b"<57>Mar 25 21:47:46 gleichner6005 quaerat[2444]: There were "[..]);
        let result = decoder.decode(&mut buffer);
        assert_eq!(Ok(None), result.map_err(|_| true));

        buffer.put(&b"8 penguins in the shop.\n"[..]);
        let result = decoder.decode(&mut buffer);
        assert_eq!(
            Ok(Some("<57>Mar 25 21:47:46 gleichner6005 quaerat[2444]: There were 8 penguins in the shop.".into())),
            result.map_err(|_| true)
        );
    }

    #[test]
    fn octet_decode_works_with_multiple_frames() {
        let mut decoder = OctetCountingDecoder::new_with_max_length(30);
        let mut buffer = BytesMut::with_capacity(16);

        buffer.put(&b"28 abcdefghijklm"[..]);
        let result = decoder.decode(&mut buffer);
        assert_eq!(Ok(None), result.map_err(|_| false));

        // Sending another frame starting with a number should not cause it to
        // try to decode a new message.
        buffer.put(&b"3 nopqrstuvwxyz"[..]);
        let result = decoder.decode(&mut buffer);
        assert_eq!(
            Ok(Some("abcdefghijklm3 nopqrstuvwxyz".into())),
            result.map_err(|_| false)
        );
    }

    #[test]
    fn octet_decode_moves_past_invalid_length() {
        let mut decoder = OctetCountingDecoder::new_with_max_length(16);
        let mut buffer = BytesMut::with_capacity(16);

        // An invalid syslog message that starts with a digit so we think it is starting with the len.
        buffer.put(&b"232>1 zork"[..]);
        let result = decoder.decode(&mut buffer);

        assert!(result.is_err());
        assert_eq!(b"zork"[..], buffer);
    }

    #[test]
    fn octet_decode_moves_past_invalid_utf8() {
        let mut decoder = OctetCountingDecoder::new_with_max_length(16);
        let mut buffer = BytesMut::with_capacity(16);

        // An invalid syslog message containing invalid utf8 bytes.
        buffer.put(&[b'4', b' ', 0xf0, 0x28, 0x8c, 0xbc][..]);
        let result = decoder.decode(&mut buffer);

        assert!(result.is_err());
        assert_eq!(b""[..], buffer);
    }

    #[test]
    fn octet_decode_moves_past_exceeded_frame_length() {
        let mut decoder = OctetCountingDecoder::new_with_max_length(16);
        let mut buffer = BytesMut::with_capacity(32);

        buffer.put(&b"32thisshouldbelongerthanthmaxframeasizewhichmeansthesyslogparserwillnotbeabletodecodeit\n"[..]);
        let result = decoder.decode(&mut buffer);

        assert!(result.is_err());
        assert_eq!(b""[..], buffer);
    }

    #[test]
    fn octet_decode_rejects_exceeded_frame_length() {
        let mut decoder = OctetCountingDecoder::new_with_max_length(16);
        let mut buffer = BytesMut::with_capacity(32);

        buffer.put(&b"26 abcdefghijklmnopqrstuvwxyzand here we are"[..]);
        let result = decoder.decode(&mut buffer);
        assert_eq!(Ok(None), result.map_err(|_| false));
        let result = decoder.decode(&mut buffer);

        assert!(result.is_err());
        assert_eq!(b"and here we are"[..], buffer);
    }

    #[test]
    fn octet_decode_rejects_exceeded_frame_length_multiple_frames() {
        let mut decoder = OctetCountingDecoder::new_with_max_length(16);
        let mut buffer = BytesMut::with_capacity(32);

        buffer.put(&b"26 abc"[..]);
        let _result = decoder.decode(&mut buffer);

        buffer.put(&b"defghijklmnopqrstuvwxyzand here we are"[..]);
        let result = decoder.decode(&mut buffer);

        println!("{:?}", result);
        assert!(result.is_err());
        assert_eq!(b"and here we are"[..], buffer);
    }

    #[test]
    fn octet_decode_moves_past_exceeded_frame_length_multiple_frames() {
        let mut decoder = OctetCountingDecoder::new_with_max_length(16);
        let mut buffer = BytesMut::with_capacity(32);

        buffer.put(&b"32thisshouldbelongerthanthmaxframeasizewhichmeansthesyslogparserwillnotbeabletodecodeit"[..]);
        _ = decoder.decode(&mut buffer);

        assert_eq!(decoder.octet_decoding, Some(State::DiscardingToEol));
        buffer.put(&b"wemustcontinuetodiscard\n32 something valid"[..]);
        let result = decoder.decode(&mut buffer);

        assert!(result.is_err());
        assert_eq!(b"32 something valid"[..], buffer);
    }
}
