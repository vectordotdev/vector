use std::io;


pub const DEFAULT_COMPRESSION_LEVEL: i32 = zstd::DEFAULT_COMPRESSION_LEVEL;

pub struct ZstdEncoder<W: io::Write> {
    inner: zstd::Encoder<'static, W>,
}

impl<W: io::Write> ZstdEncoder<W> {
    pub fn new(writer: W, level: i32) -> io::Result<Self> {
        let encoder = zstd::Encoder::new(writer, level)?;
        Ok(Self {
            inner: encoder,
        })
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
unsafe impl<W: io::Write + Sync> Sync for ZstdEncoder<W> {
}
