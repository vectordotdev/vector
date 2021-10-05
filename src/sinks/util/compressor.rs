use std::io;

use flate2::write::GzEncoder;

use super::Compression;

enum Writer {
    Plain(Vec<u8>),
    Gzip(GzEncoder<Vec<u8>>),
}

impl From<Compression> for Writer {
    fn from(compression: Compression) -> Self {
        let buffer = Vec::with_capacity(1_024);
        match compression {
            Compression::None => Writer::Plain(buffer),
            Compression::Gzip(level) => Writer::Gzip(GzEncoder::new(buffer, level)),
        }
    }
}

impl io::Write for Writer {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self {
            Writer::Plain(inner_buf) => inner_buf.write(buf),
            Writer::Gzip(writer) => writer.write(buf),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        match self {
            Writer::Plain(_) => Ok(()),
            Writer::Gzip(writer) => writer.flush(),
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
    /// Consumes the compressor, returning the internal buffer used by the compressor.
    pub fn into_inner(self) -> Vec<u8> {
        match self.inner {
            Writer::Plain(buf) => buf,
            Writer::Gzip(writer) => writer
                .finish()
                .expect("gzip writer should not fail to finish"),
        }
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
