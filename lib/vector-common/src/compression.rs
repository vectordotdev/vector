use async_compression::tokio::bufread::GzipDecoder;
use tokio::io::AsyncBufRead;

#[allow(clippy::disallowed_methods)]
pub fn gzip_multiple_decoder<R: AsyncBufRead>(reader: R) -> GzipDecoder<R> {
    let mut decoder = GzipDecoder::new(reader);
    decoder.multiple_members(true);
    decoder
}
