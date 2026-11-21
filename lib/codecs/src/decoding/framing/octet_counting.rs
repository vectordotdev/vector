use std::io;

use bytes::{Buf, Bytes, BytesMut};
use memchr::memchr;
use tokio_util::codec::{Decoder, LinesCodecError};
use tracing::trace;
use vector_common::internal_event::emit;
use vector_config::configurable_component;

use super::{BoxedFramingError, CharacterDelimitedDecoder};
use crate::internal_events::DecoderFrameTooLongError;

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
    #[must_use]
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
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct OctetCountingDecoderOptions {
    /// The maximum length of the byte buffer.
    #[serde(skip_serializing_if = "vector_core::serde::is_default")]
    pub max_length: Option<usize>,
}

/// Codec using the `Octet Counting` format as specified in
/// <https://tools.ietf.org/html/rfc6587#section-3.4.1>.
#[derive(Clone, Debug)]
pub struct OctetCountingDecoder {
    other: CharacterDelimitedDecoder,
    octet_decoding: Option<State>,
    /// Whether the newline-delimited fallback is currently discarding the
    /// remainder of an oversized, non-terminated frame.
    newline_discarding: bool,
    /// How far the newline fallback has scanned for a delimiter, so
    /// partial frames are not rescanned from the start on every call.
    newline_scan_pos: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    NotDiscarding,
    Discarding(usize),
    DiscardingToEol,
}

impl OctetCountingDecoder {
    /// Creates a new `OctetCountingDecoder`.
    #[must_use]
    pub fn new() -> Self {
        Self {
            other: CharacterDelimitedDecoder::new(b'\n'),
            octet_decoding: None,
            newline_discarding: false,
            newline_scan_pos: 0,
        }
    }

    /// Creates a `OctetCountingDecoder` with a maximum frame length limit.
    #[must_use]
    pub fn new_with_max_length(max_length: usize) -> Self {
        Self {
            other: CharacterDelimitedDecoder::new_with_max_length(b'\n', max_length),
            octet_decoding: None,
            newline_discarding: false,
            newline_scan_pos: 0,
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
                Err(LinesCodecError::Io(io::Error::other(
                    "Frame length limit exceeded",
                )))
            }

            (State::Discarding(chars), _, _) => {
                // We have a certain number of chars to discard.
                //
                // There aren't enough in this frame so we need to discard the
                // entire frame and adjust the amount to discard accordingly.
                self.octet_decoding = Some(State::Discarding(chars - src.len()));
                src.advance(src.len());
                Ok(None)
            }

            (State::DiscardingToEol, Some(offset), _) => {
                // When discarding we keep discarding to the next newline.
                src.advance(offset + 1);
                self.octet_decoding = None;
                Err(LinesCodecError::Io(io::Error::other(
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
                let len: usize = if let Ok(len) = std::str::from_utf8(&src[..space_pos])
                    .map_err(|_| ())
                    .and_then(|num| num.parse().map_err(|_| ()))
                {
                    len
                } else {
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
                    // Pass raw bytes through: UTF-8 handling (strict/lossy)
                    // is up to the deserializer (syslog: lossy by default).
                    let bytes = Bytes::copy_from_slice(msg);
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
                Err(LinesCodecError::Io(io::Error::other(
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
        if let Some(&first_byte) = src.first()
            && (49..=57).contains(&first_byte)
        {
            // First character is non zero number so we can assume that
            // octet count framing is used.
            trace!("Octet counting encoded event detected.");
            self.octet_decoding = Some(State::NotDiscarding);
        }

        self.octet_decoding
            .map(|state| self.octet_decode(state, src))
    }

    /// Newline fallback on raw bytes. Enforces `max_length` incrementally,
    /// unlike the underlying decoder (which only checks at the delimiter):
    /// past the limit with no newline in sight, discard up to the next
    /// newline (as `LinesCodec` did), so a never-terminating sender cannot
    /// grow the buffer unbounded.
    fn newline_decode(&mut self, src: &mut BytesMut) -> Result<Option<Bytes>, BoxedFramingError> {
        loop {
            if self.newline_discarding {
                // Discard the remainder of the oversized frame up to its newline.
                match memchr(b'\n', src) {
                    Some(pos) => {
                        src.advance(pos + 1);
                        self.newline_discarding = false;
                        // Loop back through the length check: the bytes after
                        // the discarded frame may already be an oversized
                        // partial tail that must not wait for more reads.
                        self.newline_scan_pos = 0;
                        continue;
                    }
                    None => src.advance(src.len()),
                }
                self.newline_scan_pos = 0;
            } else {
                // The buffer may have shrunk (e.g. via `decode_eof`); clamp the cursor.
                self.newline_scan_pos = self.newline_scan_pos.min(src.len());
                if memchr(b'\n', &src[self.newline_scan_pos..]).is_none() {
                    // Partial line: only scan newly arrived bytes next call
                    // (the old `LinesCodec` `next_index` cursor).
                    self.newline_scan_pos = src.len();
                    if src.len() > self.other.max_length() {
                        // Over the limit with no delimiter: drop the buffer and
                        // discard the rest of this frame as it arrives.
                        emit(DecoderFrameTooLongError {
                            length: src.len(),
                            max_length: self.other.max_length(),
                        });
                        src.advance(src.len());
                        self.newline_discarding = true;
                        self.newline_scan_pos = 0;
                    }
                    return Ok(None);
                }
                // Complete line available; the underlying decoder rescans
                // for the delimiter once (linear, same as framing itself).
                self.newline_scan_pos = 0;
            }
            let before = src.len();
            let frame = self.other.decode(src)?;
            if frame.is_some() {
                return Ok(frame);
            }
            if src.len() == before {
                // No progress without more input.
                return Ok(None);
            }
            // The decoder skipped an oversized line and left a remainder:
            // re-check it now so an oversized partial tail enters discard
            // mode instead of waiting for (and buffering) more reads.
        }
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
            ret.map_err(Into::into)
        } else {
            // Octet counting isn't used so fallback to newline decoding on
            // raw bytes (UTF-8 is the deserializer's job; `\r` stripped for
            // `LinesCodec` parity).
            self.newline_decode(src)
                .map(|line| line.map(strip_trailing_carriage_return))
        }
    }

    fn decode_eof(&mut self, buf: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        if let Some(ret) = self.checked_decode(buf) {
            ret.map_err(Into::into)
        } else {
            // Octet counting isn't used so fallback to newline codec.
            match self.newline_decode(buf)? {
                Some(bytes) => Ok(Some(strip_trailing_carriage_return(bytes))),
                None => self.other.decode_eof(buf).map(|line| {
                    line.map(strip_trailing_carriage_return)
                        // A final remainder of exactly "\r" strips down to an
                        // empty frame; `LinesCodec` returned `None` for it.
                        .filter(|frame| !frame.is_empty())
                }),
            }
        }
    }
}

fn strip_trailing_carriage_return(mut bytes: Bytes) -> Bytes {
    if bytes.last() == Some(&b'\r') {
        bytes.truncate(bytes.len() - 1);
    }
    bytes
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
    fn octet_decode_passes_through_invalid_utf8() {
        let mut decoder = OctetCountingDecoder::new_with_max_length(16);
        let mut buffer = BytesMut::with_capacity(16);

        // An invalid syslog message containing invalid utf8 bytes: the framer
        // must pass the raw bytes through instead of dropping the frame.
        buffer.put(&[b'4', b' ', 0xf0, 0x28, 0x8c, 0xbc][..]);
        let result = decoder.decode(&mut buffer);

        assert_eq!(
            Ok(Some(Bytes::from_static(&[0xf0, 0x28, 0x8c, 0xbc]))),
            result.map_err(|_| false)
        );
        assert_eq!(b""[..], buffer);
    }

    #[test]
    fn non_octet_decode_passes_through_invalid_utf8() {
        let mut decoder = OctetCountingDecoder::new_with_max_length(128);
        let mut buffer = BytesMut::with_capacity(16);

        // Non-octet-counting (newline-delimited) message with invalid UTF-8:
        // must be returned as raw bytes, not dropped with an error.
        buffer.put(&b"<57>Mar 25 21:47:46 host app: dump "[..]);
        buffer.put(&[0x96, 0xa9, 0xea, 0xe8, 0x80, 0xff][..]);
        buffer.put(&b"\n<57>next\n"[..]);
        let result = decoder.decode(&mut buffer);
        let frame = result.unwrap().unwrap();
        assert_eq!(
            b"<57>Mar 25 21:47:46 host app: dump \x96\xa9\xea\xe8\x80\xff"[..],
            frame[..]
        );
        let next = decoder.decode(&mut buffer);
        assert_eq!(Ok(Some("<57>next".into())), next.map_err(|_| false));
    }

    #[test]
    fn non_octet_decode_strips_trailing_carriage_return() {
        let mut decoder = OctetCountingDecoder::new_with_max_length(128);
        let mut buffer = BytesMut::with_capacity(16);

        buffer.put(&b"<57>Mar 25 21:47:46 host app: msg\r\n"[..]);
        let result = decoder.decode(&mut buffer);

        assert_eq!(
            Ok(Some("<57>Mar 25 21:47:46 host app: msg".into())),
            result.map_err(|_| false)
        );
    }

    #[test]
    fn non_octet_decode_bounds_buffer_without_newline() {
        let mut decoder = OctetCountingDecoder::new_with_max_length(64);
        let mut buffer = BytesMut::with_capacity(128);

        // A sender streaming a never-terminating newline-delimited frame:
        // once the buffered bytes exceed `max_length` with no newline in
        // sight, the decoder must consume them instead of buffering
        // indefinitely (restores the incremental enforcement of the old
        // `LinesCodec` fallback).
        buffer.put(&b"<57>Mar 25 21:47:46 host app: "[..]);
        buffer.put(&vec![b'x'; 128][..]);
        assert_eq!(Ok(None), decoder.decode(&mut buffer).map_err(|_| false));
        assert!(
            buffer.is_empty(),
            "oversized unterminated frame must be consumed"
        );
        assert!(decoder.newline_discarding);

        // The rest of the oversized frame is discarded up to its newline...
        buffer.put(&b"tail of the oversized frame\n"[..]);
        assert_eq!(Ok(None), decoder.decode(&mut buffer).map_err(|_| false));

        // ...and framing resumes with the next well-formed line.
        buffer.put(&b"<57>next\n"[..]);
        assert_eq!(
            Ok(Some("<57>next".into())),
            decoder.decode(&mut buffer).map_err(|_| false)
        );
    }

    #[test]
    fn non_octet_decode_frames_after_discard_in_same_call() {
        let mut decoder = OctetCountingDecoder::new_with_max_length(64);
        let mut buffer = BytesMut::with_capacity(128);

        // Push the decoder into the discarding state with an unterminated
        // oversized frame.
        buffer.put(&vec![b'x'; 100][..]);
        assert_eq!(Ok(None), decoder.decode(&mut buffer).map_err(|_| false));
        assert!(decoder.newline_discarding);

        // The tail of the discarded frame, its newline, and a complete
        // well-formed line arriving together: framing must resume within
        // the same call.
        buffer.put(&b"tail\n<57>resumed\n"[..]);
        assert_eq!(
            Ok(Some("<57>resumed".into())),
            decoder.decode(&mut buffer).map_err(|_| false)
        );
        assert!(!decoder.newline_discarding);
    }

    #[test]
    fn non_octet_decode_drops_oversized_terminated_line() {
        let mut decoder = OctetCountingDecoder::new_with_max_length(16);
        let mut buffer = BytesMut::with_capacity(64);

        buffer.put(&b"<57>this line is way over sixteen bytes\n<57>ok\n"[..]);
        assert_eq!(
            Ok(Some("<57>ok".into())),
            decoder.decode(&mut buffer).map_err(|_| false)
        );
        assert_eq!(Ok(None), decoder.decode(&mut buffer).map_err(|_| false));
    }

    #[test]
    fn non_octet_decode_discards_oversized_partial_after_skipped_line() {
        let mut decoder = OctetCountingDecoder::new_with_max_length(64);
        let mut buffer = BytesMut::with_capacity(4200);

        // An oversized terminated line followed by an oversized partial tail:
        // skipping the line must not leave the tail buffered while waiting
        // for more reads — it enters discard mode in the same call.
        buffer.put(&vec![b'x'; 65][..]);
        buffer.put(&b"\n"[..]);
        buffer.put(&vec![b'y'; 4096][..]);
        assert_eq!(Ok(None), decoder.decode(&mut buffer).map_err(|_| false));
        assert!(buffer.is_empty(), "oversized partial tail must be consumed");
        assert!(decoder.newline_discarding);

        // The rest of the discarded frame is dropped up to its newline...
        buffer.put(&b"tail\n"[..]);
        assert_eq!(Ok(None), decoder.decode(&mut buffer).map_err(|_| false));

        // ...and framing resumes.
        buffer.put(&b"<57>ok\n"[..]);
        assert_eq!(
            Ok(Some("<57>ok".into())),
            decoder.decode(&mut buffer).map_err(|_| false)
        );
    }

    #[test]
    fn non_octet_decode_discards_oversized_tail_after_returned_frame() {
        let mut decoder = OctetCountingDecoder::new_with_max_length(16);
        let mut buffer = BytesMut::with_capacity(128);

        // An oversized line, a valid line, and an oversized partial tail in
        // one buffer: the first call returns the valid frame; the very next
        // call (no new data) must put the tail into discard mode.
        buffer.put(&b"012345678901234567890\n"[..]);
        buffer.put(&b"ok\n"[..]);
        buffer.put(&vec![b'y'; 64][..]);
        assert_eq!(
            Ok(Some("ok".into())),
            decoder.decode(&mut buffer).map_err(|_| false)
        );
        assert_eq!(Ok(None), decoder.decode(&mut buffer).map_err(|_| false));
        assert!(buffer.is_empty());
        assert!(decoder.newline_discarding);
    }

    #[test]
    fn non_octet_decode_survives_run_of_oversized_lines() {
        let mut decoder = OctetCountingDecoder::new_with_max_length(8);
        let mut buffer = BytesMut::with_capacity(256);

        // Many oversized terminated lines followed by an oversized partial
        // tail: the loop must consume them all in one call and terminate.
        for _ in 0..20 {
            buffer.put(&vec![b'x'; 9][..]);
            buffer.put(&b"\n"[..]);
        }
        buffer.put(&vec![b'y'; 100][..]);
        assert_eq!(Ok(None), decoder.decode(&mut buffer).map_err(|_| false));
        assert!(buffer.is_empty());
        assert!(decoder.newline_discarding);
    }

    #[test]
    fn non_octet_decode_accepts_exactly_max_length_line() {
        let mut decoder = OctetCountingDecoder::new_with_max_length(16);

        // A line of exactly `max_length` bytes is a valid frame.
        let mut buffer = BytesMut::from(&b"0123456789abcdef\n"[..]);
        assert_eq!(
            Ok(Some("0123456789abcdef".into())),
            decoder.decode(&mut buffer).map_err(|_| false)
        );
    }

    #[test]
    fn non_octet_decode_eof_while_discarding() {
        let mut decoder = OctetCountingDecoder::new_with_max_length(8);

        // EOF while in discard mode must not emit a frame nor panic.
        let mut buffer = BytesMut::from(&vec![b'x'; 100][..]);
        assert_eq!(Ok(None), decoder.decode(&mut buffer).map_err(|_| false));
        assert!(decoder.newline_discarding);
        assert_eq!(Ok(None), decoder.decode_eof(&mut buffer).map_err(|_| false));
    }

    #[test]
    fn non_octet_decode_rechecks_tail_after_leaving_discard_mode() {
        let mut decoder = OctetCountingDecoder::new_with_max_length(64);
        let mut buffer = BytesMut::with_capacity(4200);

        // Enter discard mode with an unterminated oversized frame.
        buffer.put(&vec![b'x'; 100][..]);
        assert_eq!(Ok(None), decoder.decode(&mut buffer).map_err(|_| false));
        assert!(decoder.newline_discarding);

        // The frame's newline followed by another oversized partial tail:
        // leaving discard mode must re-check the tail in the same call
        // instead of buffering it while waiting for more reads.
        buffer.put(&b"tail\n"[..]);
        buffer.put(&vec![b'y'; 4096][..]);
        assert_eq!(Ok(None), decoder.decode(&mut buffer).map_err(|_| false));
        assert!(buffer.is_empty());
        assert!(decoder.newline_discarding);

        // Framing resumes once the discarded tail's newline arrives.
        buffer.put(&b"tail2\n<57>ok\n"[..]);
        assert_eq!(
            Ok(Some("<57>ok".into())),
            decoder.decode(&mut buffer).map_err(|_| false)
        );
    }

    #[test]
    fn non_octet_decode_exits_discard_on_trailing_newline() {
        let mut decoder = OctetCountingDecoder::new_with_max_length(64);

        // Enter discard mode, then receive only the frame's newline: the
        // decoder leaves discard mode with an empty buffer and keeps waiting.
        let mut buffer = BytesMut::from(&vec![b'x'; 100][..]);
        assert_eq!(Ok(None), decoder.decode(&mut buffer).map_err(|_| false));
        buffer.put(&b"\n"[..]);
        assert_eq!(Ok(None), decoder.decode(&mut buffer).map_err(|_| false));
        assert!(buffer.is_empty());
        assert!(!decoder.newline_discarding);

        buffer.put(&b"<57>ok\n"[..]);
        assert_eq!(
            Ok(Some("<57>ok".into())),
            decoder.decode(&mut buffer).map_err(|_| false)
        );
    }

    #[test]
    fn non_octet_decode_keeps_scan_cursor_across_partial_reads() {
        let mut decoder = OctetCountingDecoder::new_with_max_length(1024);
        let mut buffer = BytesMut::with_capacity(64);

        // A line delivered across several reads must not be rescanned from
        // the start on every call (mirrors the `next_index` cursor of the
        // old `LinesCodec` fallback).
        buffer.put(&b"<57>partial"[..]);
        assert_eq!(Ok(None), decoder.decode(&mut buffer).map_err(|_| false));
        assert_eq!(decoder.newline_scan_pos, buffer.len());

        buffer.put(&b" without newline"[..]);
        assert_eq!(Ok(None), decoder.decode(&mut buffer).map_err(|_| false));
        assert_eq!(decoder.newline_scan_pos, buffer.len());

        buffer.put(&b"\n"[..]);
        let frame = decoder.decode(&mut buffer).unwrap().unwrap();
        assert_eq!(b"<57>partial without newline"[..], frame[..]);
        assert_eq!(decoder.newline_scan_pos, 0);
    }

    #[test]
    fn non_octet_decode_eof_after_partial_frame() {
        let mut decoder = OctetCountingDecoder::new_with_max_length(64);
        let mut buffer = BytesMut::with_capacity(64);

        buffer.put(&b"<57>unterminated tail"[..]);
        assert_eq!(Ok(None), decoder.decode(&mut buffer).map_err(|_| false));

        // EOF with a partial frame: the remainder is emitted as the final
        // frame, and further `decode_eof`/`decode` calls must not panic
        // with a scan cursor left beyond the (now empty) buffer.
        assert_eq!(
            Ok(Some("<57>unterminated tail".into())),
            decoder.decode_eof(&mut buffer).map_err(|_| false)
        );
        assert_eq!(Ok(None), decoder.decode_eof(&mut buffer).map_err(|_| false));
        assert_eq!(Ok(None), decoder.decode(&mut buffer).map_err(|_| false));
    }

    #[test]
    fn non_octet_decode_eof_carriage_return_parity() {
        let mut decoder = OctetCountingDecoder::new_with_max_length(64);

        // An unterminated remainder ending in a bare `\r` has the `\r`
        // stripped, matching `LinesCodec::decode_eof`, which applies
        // `without_carriage_return` to the final remainder as well.
        let mut buffer = BytesMut::from(&b"<57>tail\r"[..]);
        assert_eq!(
            Ok(Some("<57>tail".into())),
            decoder.decode_eof(&mut buffer).map_err(|_| false)
        );

        // A remainder that is exactly "\r" yields no frame at all, also
        // matching `LinesCodec::decode_eof`.
        let mut buffer = BytesMut::from(&b"\r"[..]);
        assert_eq!(Ok(None), decoder.decode_eof(&mut buffer).map_err(|_| false));
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

        println!("{result:?}");
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

    #[test]
    fn octet_decode_discard_partial_frame_underflow() {
        let mut decoder = OctetCountingDecoder::new_with_max_length(16);
        let mut buffer = BytesMut::with_capacity(32);

        // A length prefix of 26 exceeds the max length of 16, so the decoder
        // enters the discarding state with 26 bytes to discard, leaving "abc".
        buffer.put(&b"26 abc"[..]);
        let _result = decoder.decode(&mut buffer);
        assert_eq!(decoder.octet_decoding, Some(State::Discarding(26)));

        // Only three more bytes arrive, so the buffer holds fewer bytes than
        // remain to be discarded. This is the branch that previously underflowed.
        buffer.put(&b"def"[..]);
        let result = decoder.decode(&mut buffer);
        assert_eq!(Ok(None), result.map_err(|_| false));
        assert_eq!(decoder.octet_decoding, Some(State::Discarding(20)));
    }
}
