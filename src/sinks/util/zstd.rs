use std::{fmt::Display, io};

use super::buffer::compression::CompressionLevel;

#[derive(Debug)]
pub struct ZstdCompressionLevel(i32);

impl From<CompressionLevel> for ZstdCompressionLevel {
    fn from(value: CompressionLevel) -> Self {
        let val: i32 = match value {
            CompressionLevel::None => 0,
            CompressionLevel::Default => zstd::DEFAULT_COMPRESSION_LEVEL,
            CompressionLevel::Best => 21,
            CompressionLevel::Fast => 1,
            CompressionLevel::Val(v) => v.clamp(1, 21) as i32,
        };
        ZstdCompressionLevel(val)
    }
}

impl Display for ZstdCompressionLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

pub struct ZstdEncoder<W: io::Write> {
    inner: zstd::Encoder<'static, W>,
}

impl<W: io::Write> ZstdEncoder<W> {
    pub fn new(writer: W, level: ZstdCompressionLevel) -> io::Result<Self> {
        let encoder = zstd::Encoder::new(writer, level.0)?;
        Ok(Self { inner: encoder })
    }

    pub fn finish(self) -> io::Result<W> {
        self.inner.finish()
    }

    pub fn get_ref(&self) -> &W {
        self.inner.get_ref()
    }
}

impl<W: io::Write> io::Write for ZstdEncoder<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        #[allow(clippy::disallowed_methods)] // Caller handles the result of `write`.
        self.inner.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

impl<W: io::Write + std::fmt::Debug> std::fmt::Debug for ZstdEncoder<W> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ZstdEncoder")
            .field("inner", &self.get_ref())
            .finish()
    }
}

/// Safety:
/// 1. There is no sharing references to zstd encoder. `Write` requires unique reference, and `finish` moves the instance itself.
/// 2. Sharing only internal writer, which implements `Sync`
unsafe impl<W: io::Write + Sync> Sync for ZstdEncoder<W> {}
