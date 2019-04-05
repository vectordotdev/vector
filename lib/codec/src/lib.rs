extern crate bytes;
extern crate tokio_codec;

#[macro_use]
extern crate tokio_trace;

use bytes::{BufMut, Bytes, BytesMut};
use std::{cmp, fmt, io, usize};
use tokio_codec::{Decoder, Encoder};

#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct BytesDelimitedCodec {
    delim: u8,
    max_length: usize,
    is_discarding: bool,
    next_index: usize,
}

#[derive(Debug)]
pub enum Error {
    MaxLimitExceeded,
    Io(io::Error),
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

    fn discard(&mut self, newline_offset: Option<usize>, read_to: usize, buf: &mut BytesMut) {
        let discard_to = if let Some(offset) = newline_offset {
            // If we found a newline, discard up to that offset and
            // then stop discarding. On the next iteration, we'll try
            // to read a line normally.
            self.is_discarding = false;
            offset + self.next_index + 1
        } else {
            // Otherwise, we didn't find a newline, so we'll discard
            // everything we read. On the next iteration, we'll continue
            // discarding up to max_len bytes unless we find a newline.
            read_to
        };
        buf.advance(discard_to);
        self.next_index = 0;
    }
}

impl Decoder for BytesDelimitedCodec {
    type Item = Bytes;
    type Error = Error;

    fn decode(&mut self, buf: &mut BytesMut) -> Result<Option<Bytes>, Error> {
        loop {
            // Determine how far into the buffer we'll search for a newline. If
            // there's no max_length set, we'll read to the end of the buffer.
            let read_to = cmp::min(self.max_length.saturating_add(1), buf.len());

            let newline_pos = buf[self.next_index..read_to]
                .iter()
                .position(|b| *b == self.delim);

            if self.is_discarding {
                self.discard(newline_pos, read_to, buf);
            } else {
                return if let Some(pos) = newline_pos {
                    // We found a correct frame

                    let newpos_index = pos + self.next_index;
                    self.next_index = 0;
                    let frame = buf.split_to(newpos_index + 1);

                    trace!(
                        message = "decoding the frame.",
                        bytes_proccesed = frame.len()
                    );

                    let frame = &frame[..frame.len() - 1];

                    Ok(Some(frame.into()))
                } else if buf.len() > self.max_length {
                    // We reached the max length without finding the
                    // delimiter so must discard the rest until we
                    // reach the next delimiter
                    self.is_discarding = true;
                    Err(Error::MaxLimitExceeded)
                } else {
                    // We didn't find the delimiter and didn't
                    // reach the max frame length.
                    self.next_index = read_to;
                    Ok(None)
                };
            }
        }
    }

    fn decode_eof(&mut self, buf: &mut BytesMut) -> Result<Option<Bytes>, Error> {
        let frame = match self.decode(buf)? {
            Some(frame) => Some(frame),
            None if !buf.is_empty() => {
                let frame = buf.take();
                self.next_index = 0;

                Some(frame.into())
            }
            _ => None,
        };

        Ok(frame)
    }
}

impl Encoder for BytesDelimitedCodec {
    type Item = Bytes;
    type Error = Error;

    fn encode(&mut self, item: Bytes, buf: &mut BytesMut) -> Result<(), Error> {
        buf.reserve(item.len() + 1);
        buf.put(item);
        buf.put_u8(self.delim);
        Ok(())
    }
}

impl From<io::Error> for Error {
    fn from(io: io::Error) -> Self {
        Error::Io(io)
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Error::MaxLimitExceeded => write!(f, "line length limit exceeded"),
            Error::Io(io) => io.fmt(f),
        }
    }
}

impl ::std::error::Error for Error {}
