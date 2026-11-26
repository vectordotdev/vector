use async_compression::tokio::bufread::{GzipDecoder, ZstdDecoder};
use tokio::io::AsyncBufRead;

use crate::constants::ZSTD_MAGIC;

// Zstd skippable frames dont' start with ZSTD_MAGIC. We check for them separately.
#[must_use]
pub fn is_zstd_header(header: &[u8]) -> bool {
    header.starts_with(ZSTD_MAGIC) || matches!(header, [0x50..=0x5f, 0x2a, 0x4d, 0x18, ..])
}

#[allow(clippy::disallowed_methods)]
pub fn gzip_multiple_decoder<R: AsyncBufRead>(reader: R) -> GzipDecoder<R> {
    let mut decoder = GzipDecoder::new(reader);
    decoder.multiple_members(true);
    decoder
}

#[allow(clippy::disallowed_methods)]
pub fn zstd_multiple_decoder<R: AsyncBufRead>(reader: R) -> ZstdDecoder<R> {
    let mut decoder = ZstdDecoder::new(reader);
    decoder.multiple_members(true);
    decoder
}
