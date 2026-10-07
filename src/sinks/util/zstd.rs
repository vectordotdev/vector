use std::{fmt::Display, io};

use super::buffer::compression::CompressionLevel;

#[derive(Debug)]
pub struct ZstdCompressionLevel(i32);

impl From<CompressionLevel> for ZstdCompressionLevel {
    // https://github.com/vectordotdev/vector/issues/23659
    #[allow(
        clippy::cast_possible_wrap,
        reason = "Preserve the existing numeric conversion until signed overflow behavior is audited."
    )]
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
    // https://github.com/vectordotdev/vector/issues/23659
    #[allow(
        clippy::missing_errors_doc,
        reason = "Audit and document the existing error contracts separately from lint enforcement."
    )]
    #[allow(
        clippy::needless_pass_by_value,
        reason = "Keep ownership and drop timing unchanged during the lint rollout."
    )]
    pub fn new(writer: W, level: ZstdCompressionLevel) -> io::Result<Self> {
        let encoder = zstd::Encoder::new(writer, level.0)?;
        Ok(Self { inner: encoder })
    }

    // https://github.com/vectordotdev/vector/issues/23659
    #[allow(
        clippy::missing_errors_doc,
        reason = "Audit and document the existing error contracts separately from lint enforcement."
    )]
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
