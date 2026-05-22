use std::io::Read;

use bytes::{Buf, Bytes};
use flate2::read::{MultiGzDecoder, ZlibDecoder};
use snap::raw::Decoder as SnappyDecoder;
use warp::http::StatusCode;

use crate::{common::http::ErrorMessage, internal_events::HttpDecompressError};

/// Default cap on the decompressed body size produced by [`decompress_body`].
///
/// Prevents a compressed "bomb" payload from causing unbounded memory growth.
pub(crate) const DEFAULT_MAX_DECOMPRESSED_BODY_SIZE: usize = 100 * 1024 * 1024;

/// Decompresses the body based on the Content-Encoding header.
///
/// Supports gzip, deflate, snappy, zstd, and identity (no compression).
///
/// Caps the decompressed output at 100 MiB to mitigate decompression-bomb DoS attacks.
pub fn decompress_body(header: Option<&str>, body: Bytes) -> Result<Bytes, ErrorMessage> {
    decompress_body_with_limit(header, body, Some(DEFAULT_MAX_DECOMPRESSED_BODY_SIZE))
}

/// Like [`decompress_body`], but allows the caller to control the decompressed size cap.
///
/// `max_decompressed_size = None` disables the cap (not recommended for unauthenticated input).
pub(crate) fn decompress_body_with_limit(
    header: Option<&str>,
    mut body: Bytes,
    max_decompressed_size: Option<usize>,
) -> Result<Bytes, ErrorMessage> {
    if let Some(encodings) = header {
        for encoding in encodings.rsplit(',').map(str::trim) {
            body = match encoding {
                "identity" => body,
                "gzip" => decompress_reader(
                    MultiGzDecoder::new(body.reader()),
                    encoding,
                    max_decompressed_size,
                )?,
                "deflate" => decompress_reader(
                    ZlibDecoder::new(body.reader()),
                    encoding,
                    max_decompressed_size,
                )?,
                "snappy" => decompress_snappy(&body, max_decompressed_size)?,
                "zstd" => {
                    let decoder = zstd::stream::read::Decoder::new(body.reader())
                        .map_err(|error| emit_decompress_error(encoding, error))?;
                    decompress_reader(decoder, encoding, max_decompressed_size)?
                }
                encoding => {
                    return Err(ErrorMessage::new(
                        StatusCode::UNSUPPORTED_MEDIA_TYPE,
                        format!("Unsupported encoding {encoding}"),
                    ));
                }
            }
        }
    }

    Ok(body)
}

fn decompress_reader<R: Read>(
    reader: R,
    encoding: &str,
    max_decompressed_size: Option<usize>,
) -> Result<Bytes, ErrorMessage> {
    let mut decoded = Vec::new();
    match max_decompressed_size {
        Some(max) => {
            // Read one byte beyond the cap so we can detect overflow without ambiguity.
            let limit = u64::try_from(max).unwrap_or(u64::MAX).saturating_add(1);
            reader
                .take(limit)
                .read_to_end(&mut decoded)
                .map_err(|error| emit_decompress_error(encoding, error))?;
            if decoded.len() > max {
                return Err(decompressed_too_large_error(encoding, max));
            }
        }
        None => {
            let mut reader = reader;
            reader
                .read_to_end(&mut decoded)
                .map_err(|error| emit_decompress_error(encoding, error))?;
        }
    }
    Ok(decoded.into())
}

fn decompress_snappy(
    body: &Bytes,
    max_decompressed_size: Option<usize>,
) -> Result<Bytes, ErrorMessage> {
    // Snappy stores the decompressed length in the frame header, so reject oversized
    // payloads before allocating the output buffer.
    if let Some(max) = max_decompressed_size {
        let len = snap::raw::decompress_len(body)
            .map_err(|error| emit_decompress_error("snappy", error))?;
        if len > max {
            return Err(decompressed_too_large_error("snappy", max));
        }
    }
    let decoded = SnappyDecoder::new()
        .decompress_vec(body)
        .map_err(|error| emit_decompress_error("snappy", error))?;
    Ok(decoded.into())
}

fn decompressed_too_large_error(encoding: &str, max: usize) -> ErrorMessage {
    ErrorMessage::new(
        StatusCode::PAYLOAD_TOO_LARGE,
        format!("Decompressed {encoding} body exceeds limit of {max} bytes."),
    )
}

pub fn emit_decompress_error(encoding: &str, error: impl std::error::Error) -> ErrorMessage {
    emit!(HttpDecompressError {
        encoding,
        error: &error
    });
    ErrorMessage::new(
        StatusCode::UNPROCESSABLE_ENTITY,
        format!("Failed decompressing payload with {encoding} decoder."),
    )
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use flate2::{Compression, write::GzEncoder};

    use super::*;

    fn gzip_payload(plaintext: &[u8]) -> Bytes {
        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(plaintext).unwrap();
        encoder.finish().unwrap().into()
    }

    #[test]
    fn gzip_within_limit_succeeds() {
        let plaintext = vec![0u8; 10_000];
        let body = gzip_payload(&plaintext);

        let decoded = decompress_body_with_limit(Some("gzip"), body, Some(100_000)).unwrap();
        assert_eq!(decoded.len(), plaintext.len());
    }

    #[test]
    fn gzip_exceeding_limit_returns_413() {
        // Compress 1 MB of zeros, then cap at 1 KB.
        let plaintext = vec![0u8; 1_000_000];
        let body = gzip_payload(&plaintext);

        let err =
            decompress_body_with_limit(Some("gzip"), body, Some(1024)).expect_err("should reject");
        assert_eq!(err.status_code(), StatusCode::PAYLOAD_TOO_LARGE);
    }

    #[test]
    fn default_limit_protects_against_decompression_bomb() {
        // A small input that would expand far past 100 MB if we let it run unbounded.
        let plaintext = vec![0u8; 200 * 1024 * 1024];
        let body = gzip_payload(&plaintext);

        let err = decompress_body(Some("gzip"), body).expect_err("should reject");
        assert_eq!(err.status_code(), StatusCode::PAYLOAD_TOO_LARGE);
    }

    #[test]
    fn snappy_exceeding_limit_returns_413_before_allocating() {
        // 2 MB of zeros. Snappy keeps the embedded length in the frame header.
        let plaintext = vec![0u8; 2 * 1024 * 1024];
        let compressed = snap::raw::Encoder::new().compress_vec(&plaintext).unwrap();

        let err = decompress_body_with_limit(Some("snappy"), compressed.into(), Some(1024))
            .expect_err("should reject");
        assert_eq!(err.status_code(), StatusCode::PAYLOAD_TOO_LARGE);
    }

    #[test]
    fn identity_passes_through() {
        let body: Bytes = Bytes::from_static(b"hello world");
        let decoded = decompress_body(Some("identity"), body.clone()).unwrap();
        assert_eq!(decoded, body);
    }
}
