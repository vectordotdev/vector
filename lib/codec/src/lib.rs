#[macro_use]
extern crate tracing;

use bytes::{Buf, BufMut, Bytes, BytesMut};
use std::{cmp, io, usize};
use tokio_util::codec::{Decoder, Encoder};

#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct BytesDelimitedCodec {
    delim: u8,
    max_length: usize,
    is_discarding: bool,
    next_index: usize,
}

impl BytesDelimitedCodec {
    /// Returns a `BytesDelimitedCodec` with the specified delimiter.
    pub fn new(delim: u8) -> Self {
        BytesDelimitedCodec {
            delim,
            max_length: usize::MAX,
            is_discarding: false,
            next_index: 0,
        }
    }

    /// Returns a `BytesDelimitedCodec` with a maximum frame length limit.
    pub fn new_with_max_length(delim: u8, max_length: usize) -> Self {
        BytesDelimitedCodec {
            max_length,
            ..BytesDelimitedCodec::new(delim)
        }
    }

    /// Returns the maximum frame length when decoding.
    pub fn max_length(&self) -> usize {
        self.max_length
    }
}

impl Decoder for BytesDelimitedCodec {
    type Item = Bytes;
    type Error = io::Error;

    fn decode(&mut self, buf: &mut BytesMut) -> Result<Option<Bytes>, io::Error> {
        loop {
            // Determine how far into the buffer we'll search for a newline. If
            // there's no max_length set, we'll read to the end of the buffer.
            let read_to = cmp::min(self.max_length.saturating_add(1), buf.len());

            let newline_pos = buf[self.next_index..read_to]
                .iter()
                .position(|b| *b == self.delim);

            match (self.is_discarding, newline_pos) {
                (true, Some(offset)) => {
                    // If we found a newline, discard up to that offset and
                    // then stop discarding. On the next iteration, we'll try
                    // to read a line normally.
                    buf.advance(offset + self.next_index + 1);
                    self.is_discarding = false;
                    self.next_index = 0;
                }
                (true, None) => {
                    // Otherwise, we didn't find a newline, so we'll discard
                    // everything we read. On the next iteration, we'll continue
                    // discarding up to max_len bytes unless we find a newline.
                    buf.advance(read_to);
                    self.next_index = 0;
                    if buf.is_empty() {
                        return Ok(None);
                    }
                }
                (false, Some(pos)) => {
                    // We found a correct frame

                    let newpos_index = pos + self.next_index;
                    self.next_index = 0;
                    let mut frame = buf.split_to(newpos_index + 1);

                    trace!(
                        message = "Decoding the frame.",
                        bytes_proccesed = frame.len()
                    );

                    let frame = frame.split_to(frame.len() - 1);

                    return Ok(Some(frame.freeze()));
                }
                (false, None) if buf.len() > self.max_length => {
                    // We reached the max length without finding the
                    // delimiter so must discard the rest until we
                    // reach the next delimiter
                    self.is_discarding = true;
                    warn!(
                        message = "Discarding frame larger than max_length.",
                        buf_len = buf.len(),
                        max_length = self.max_length,
                        internal_log_rate_secs = 30
                    );
                    return Ok(None);
                }
                (false, None) => {
                    // We didn't find the delimiter and didn't
                    // reach the max frame length.
                    self.next_index = read_to;
                    return Ok(None);
                }
            }
        }
    }

    fn decode_eof(&mut self, buf: &mut BytesMut) -> Result<Option<Bytes>, io::Error> {
        let frame = match self.decode(buf)? {
            Some(frame) => Some(frame),
            None if !buf.is_empty() && !self.is_discarding => {
                let frame = buf.split_to(buf.len());
                self.next_index = 0;

                Some(frame.into())
            }
            _ => None,
        };

        Ok(frame)
    }
}

impl<T> Encoder<T> for BytesDelimitedCodec
where
    T: AsRef<[u8]>,
{
    type Error = io::Error;

    fn encode(&mut self, item: T, buf: &mut BytesMut) -> Result<(), io::Error> {
        let item = item.as_ref();
        buf.reserve(item.len() + 1);
        buf.put(item);
        buf.put_u8(self.delim);
        Ok(())
    }
}
