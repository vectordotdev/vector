//! An encoder for [Snappy] compression.
//! Whilst there does exist a [Writer] implementation for Snappy, this compresses
//! using the [Snappy frame format][frame], which is not quite what we want. So
//! instead this encoder buffers the data in a [`Vec`] until the end. The `raw`
//! compressor is then used to compress the data and writes it to the provided
//! writer.
//!
//! [Snappy]: https://github.com/google/snappy/blob/main/docs/README.md
//! [Writer]: https://docs.rs/snap/latest/snap/write/struct.FrameEncoder.html
//! [frame]: https://github.com/google/snappy/blob/master/framing_format.txt

use std::io;

use snap::raw::Encoder;

pub struct SnappyEncoder<W: io::Write> {
    writer: W,
    buffer: Vec<u8>,
}

impl<W: io::Write> SnappyEncoder<W> {
    pub fn new(writer: W) -> Self {
        Self {
            writer,
            buffer: Vec::new(),
        }
    }

    pub fn finish(mut self) -> io::Result<W> {
        let mut encoder = Encoder::new();
        let compressed = encoder.compress_vec(&self.buffer)?;

        self.writer.write_all(&compressed)?;

        Ok(self.writer)
    }

    pub const fn get_ref(&self) -> &W {
        &self.writer
    }

    pub fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }
}

impl<W: io::Write> io::Write for SnappyEncoder<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.buffer.extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl<W: io::Write + std::fmt::Debug> std::fmt::Debug for SnappyEncoder<W> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SnappyEncoder")
            .field("inner", &self.get_ref())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use bytes::{BufMut, BytesMut};

    use super::*;

    #[test]
    fn is_empty() {
        let writer = BytesMut::with_capacity(64).writer();
        let mut encoder = SnappyEncoder::new(writer);

        encoder.write_all(b"I am a potato").unwrap();

        // Because we are buffering the results until the end, the writer will be
        // empty, but our buffer won't be. The `is_empty` function is provided to
        // allow us to determine if data has been written to the encoder without having
        // to check the writer.
        assert!(encoder.get_ref().get_ref().is_empty());
        assert!(!encoder.is_empty());
    }
}
