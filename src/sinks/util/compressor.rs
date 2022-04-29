use std::io;

use bytes::{BufMut, BytesMut};
use flate2::write::{GzEncoder, ZlibEncoder};

use super::Compression;

enum Writer {
    Plain(bytes::buf::Writer<BytesMut>),
    Gzip(GzEncoder<bytes::buf::Writer<BytesMut>>),
    Zlib(ZlibEncoder<bytes::buf::Writer<BytesMut>>),
}

impl Writer {
    pub fn get_ref(&self) -> &BytesMut {
        match self {
            Writer::Plain(inner) => inner.get_ref(),
            Writer::Gzip(inner) => inner.get_ref().get_ref(),
            Writer::Zlib(inner) => inner.get_ref().get_ref(),
        }
    }
}

impl From<Compression> for Writer {
    fn from(compression: Compression) -> Self {
        let writer = BytesMut::with_capacity(1_024).writer();
        match compression {
            Compression::None => Writer::Plain(writer),
            Compression::Gzip(level) => Writer::Gzip(GzEncoder::new(writer, level)),
        }
    }
}

impl io::Write for Writer {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self {
            Writer::Plain(inner_buf) => inner_buf.write(buf),
            Writer::Gzip(writer) => writer.write(buf),
            Writer::Zlib(writer) => writer.write(buf),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        match self {
            Writer::Plain(writer) => writer.flush(),
            Writer::Gzip(writer) => writer.flush(),
            Writer::Zlib(writer) => writer.flush(),
        }
    }
}

/// Simple compressor implementation based on [`Compression`].
///
/// Users can acquire a `Compressor` via [`Compressor::from`] based on the desired compression scheme.
pub struct Compressor {
    inner: Writer,
}

impl Compressor {
    /// Creates a zlib-based compressor with the default compression level.
    pub fn zlib_default() -> Self {
        let buf = BytesMut::with_capacity(1_024);
        Self {
            inner: Writer::Zlib(ZlibEncoder::new(
                buf.writer(),
                flate2::Compression::default(),
            )),
        }
    }

    pub fn get_ref(&self) -> &BytesMut {
        self.inner.get_ref()
    }

    /// Consumes the compressor, returning the internal buffer used by the compressor.
    ///
    /// # Errors
    ///
    /// If the compressor encounters an I/O error while finalizing the payload, an error
    /// variant will be returned.
    pub fn finish(self) -> io::Result<BytesMut> {
        let buf = match self.inner {
            Writer::Plain(writer) => writer,
            Writer::Gzip(writer) => writer.finish()?,
            Writer::Zlib(writer) => writer.finish()?,
        }
        .into_inner();

        Ok(buf)
    }

    /// Consumes the compressor, returning the internal buffer used by the compressor.
    ///
    /// # Panics
    ///
    /// Panics if finalizing the compressor encounters an I/O error.  This should generally only be
    /// possible when the system is out of memory and allocations cannot be performed to write any
    /// footer/checksum data.
    ///
    /// Consider using `finish` if catching these scenarios is important.
    pub fn into_inner(self) -> BytesMut {
        match self.inner {
            Writer::Plain(writer) => writer,
            Writer::Gzip(writer) => writer
                .finish()
                .expect("gzip writer should not fail to finish"),
            Writer::Zlib(writer) => writer
                .finish()
                .expect("zlib writer should not fail to finish"),
        }
        .into_inner()
    }
}

impl io::Write for Compressor {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.inner.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

impl From<Compression> for Compressor {
    fn from(compression: Compression) -> Self {
        Compressor {
            inner: compression.into(),
        }
    }
}
