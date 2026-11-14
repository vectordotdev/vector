use async_compression::tokio::bufread::{GzipDecoder, ZstdDecoder};
use tokio::io::AsyncBufRead;

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
