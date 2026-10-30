use std::{fmt::Display, io};

use super::buffer::compression::CompressionLevel;

#[derive(Clone, Copy, Debug)]
pub struct ZstdCompressionLevel(i32);

impl From<CompressionLevel> for ZstdCompressionLevel {
    fn from(value: CompressionLevel) -> Self {
        let val: i32 = match value {
            CompressionLevel::None => 0,
            CompressionLevel::Default => zstd::DEFAULT_COMPRESSION_LEVEL,
            CompressionLevel::Best => 21,
            CompressionLevel::Fast => 1,
            CompressionLevel::Val(v) => v.clamp(1, 21).cast_signed(),
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
    /// Creates a compression encoder.
    ///
    /// # Errors
    ///
    /// Returns an error if the encoder cannot be initialized.
    pub fn new(
        writer: W,
        level: ZstdCompressionLevel,
        window_log: Option<u32>,
    ) -> io::Result<Self> {
        let mut encoder = zstd::Encoder::new(writer, level.0)?;
        if let Some(window_log) = window_log {
            encoder.window_log(window_log)?;
        }
        Ok(Self { inner: encoder })
    }

    /// Finishes compression and returns the writer.
    ///
    /// # Errors
    ///
    /// Returns an error if finalizing the compressed stream fails.
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

#[cfg(test)]
mod tests {
    use std::io::Write;

    use vector_common::decompression::CappedDecoder;

    use super::*;

    #[test]
    fn window_log_sets_the_frame_window() {
        let data = vec![b'a'; 1 << 20];
        let mut encoder =
            ZstdEncoder::new(Vec::new(), CompressionLevel::Default.into(), Some(27)).unwrap();
        encoder.write_all(&data).unwrap();
        let frame = encoder.finish().unwrap();

        // A limit of 2^26 also caps the decoder window at 2^26: rejected there and accepted at
        // 2^27, so the frame's window is exactly 2^27.
        let capped = CappedDecoder::zstd_with_limit(frame.as_slice(), 1 << 26).unwrap();
        assert!(capped.decompress().is_err());

        let decoder = CappedDecoder::zstd_with_limit(frame.as_slice(), 1 << 27).unwrap();
        assert_eq!(decoder.decompress().unwrap(), data);
    }
}
