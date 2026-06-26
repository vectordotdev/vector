use bytes::{Buf, BufMut, Bytes, BytesMut};
use futures_util::StreamExt;
use snap::raw::Decoder as SnappyDecoder;
use warp::http::StatusCode;
use warp::{Filter, filters::BoxedFilter};

#[cfg(test)]
use crate::sources::util::decompression::DEFAULT_MAX_DECOMPRESSED_SIZE_BYTES;
pub use crate::sources::util::decompression::set_max_decompressed_size_bytes;
use crate::sources::util::decompression::{
    CappedDecoder, is_decompressed_size_limit_error, max_decompressed_size_bytes,
};
use crate::{common::http::ErrorMessage, internal_events::HttpDecompressError};

/// Collects a request body into [`Bytes`] while enforcing an in-memory size cap.
///
/// The cap is the global decompressed-size limit ([`max_decompressed_size_bytes`]): it bounds the
/// raw (still-compressed) body a source buffers before decompression, so a large upload cannot
/// drive unbounded allocation independently of the decompressed-size cap.
pub(crate) fn capped_body() -> BoxedFilter<(Bytes,)> {
    let max_body_size = max_decompressed_size_bytes();
    let max_body_size_header = u64::try_from(max_body_size).unwrap_or(u64::MAX);

    warp::header::optional::<u64>("content-length")
        .and_then(move |declared: Option<u64>| async move {
            if declared.is_some_and(|len| len > max_body_size_header) {
                Err(warp::reject::custom(request_body_too_large_error(
                    max_body_size,
                )))
            } else {
                Ok(())
            }
        })
        .untuple_one()
        .and(warp::body::stream())
        .and_then(move |body| async move {
            collect_body_with_limit(body, max_body_size)
                .await
                .map_err(warp::reject::custom)
        })
        .boxed()
}

/// Decompresses the body based on the Content-Encoding header.
///
/// Supports gzip, deflate, snappy, zstd, and identity (no compression).
///
/// Caps the decompressed output at 100 MiB to mitigate decompression-bomb DoS attacks.
pub fn decompress_body(header: Option<&str>, body: Bytes) -> Result<Bytes, ErrorMessage> {
    decompress_body_with_limit(header, body, max_decompressed_size_bytes())
}

/// Like [`decompress_body`], but allows the caller to control the decompressed size cap.
pub(crate) fn decompress_body_with_limit(
    header: Option<&str>,
    mut body: Bytes,
    max_decompressed_size: usize,
) -> Result<Bytes, ErrorMessage> {
    if let Some(encodings) = header {
        for encoding in encodings.rsplit(',').map(str::trim) {
            body = match encoding {
                "identity" => body,
                "gzip" => CappedDecoder::gzip_with_limit(body.reader(), max_decompressed_size)
                    .decompress()
                    .map(Bytes::from)
                    .map_err(|error| {
                        emit_decompress_error(encoding, error, max_decompressed_size)
                    })?,
                "deflate" => CappedDecoder::zlib_with_limit(body.reader(), max_decompressed_size)
                    .decompress()
                    .map(Bytes::from)
                    .map_err(|error| {
                        emit_decompress_error(encoding, error, max_decompressed_size)
                    })?,
                "snappy" => decompress_snappy(&body, max_decompressed_size)?,
                "zstd" => CappedDecoder::zstd_http_with_limit(body.reader(), max_decompressed_size)
                    .map_err(|error| emit_decompress_error(encoding, error, max_decompressed_size))?
                    .decompress()
                    .map(Bytes::from)
                    .map_err(|error| {
                        emit_decompress_error(encoding, error, max_decompressed_size)
                    })?,
                encoding => {
                    return Err(ErrorMessage::new(
                        StatusCode::UNSUPPORTED_MEDIA_TYPE,
                        format!("Unsupported encoding {encoding}"),
                    ));
                }
            }
        }
    }

    ensure_body_within_limit(&body, "identity", max_decompressed_size)?;
    Ok(body)
}

fn decompress_snappy(body: &Bytes, max_decompressed_size: usize) -> Result<Bytes, ErrorMessage> {
    // Snappy stores the decompressed length in the frame header, so reject oversized
    // payloads before allocating the output buffer.
    let len = snap::raw::decompress_len(body).map_err(|error| {
        emit_decompress_error(
            "snappy",
            std::io::Error::other(error),
            max_decompressed_size,
        )
    })?;
    if len > max_decompressed_size {
        return Err(decompressed_too_large_error(
            "snappy",
            max_decompressed_size,
        ));
    }
    let decoded = SnappyDecoder::new().decompress_vec(body).map_err(|error| {
        emit_decompress_error(
            "snappy",
            std::io::Error::other(error),
            max_decompressed_size,
        )
    })?;
    Ok(decoded.into())
}

/// Spare capacity added to the initial buffer so a third or later chunk can be appended without reallocating right away.
const ADDITIONAL_CAPACITY_FOR_CHUNKS_BEYOND_FIRST_TWO: usize = 16 * 1024;

/// Collects the body into [`Bytes`] under `max_body_size`, mirroring the fast
/// paths of hyper `to_bytes`. Single-chunk bodies avoid the `BytesMut`
/// allocation: a buffer sized for both chunks plus an arbitrary 16 KiB (to try
/// to avoid having to reallocate multiple times once other chunks arrive) is
/// only allocated once a second chunk arrives.
/// (<https://github.com/hyperium/hyper/blob/v0.14.32/src/body/to_bytes.rs>).
async fn collect_body_with_limit<S, B>(body: S, max_body_size: usize) -> Result<Bytes, ErrorMessage>
where
    S: futures_util::Stream<Item = Result<B, warp::Error>>,
    B: Buf,
{
    futures_util::pin_mut!(body);

    let mut total_body_size: usize = 0;
    let mut admit_chunk_within_limit = |chunk: Result<B, warp::Error>| -> Result<B, ErrorMessage> {
        let chunk = chunk.map_err(|error| {
            ErrorMessage::new(
                StatusCode::BAD_REQUEST,
                format!("Failed reading request body: {error}"),
            )
        })?;

        total_body_size = total_body_size.saturating_add(chunk.remaining());
        if total_body_size > max_body_size {
            return Err(request_body_too_large_error(max_body_size));
        }

        Ok(chunk)
    };

    let Some(chunk) = body.next().await else {
        return Ok(Bytes::new());
    };
    let mut first = admit_chunk_within_limit(chunk)?;

    let Some(chunk) = body.next().await else {
        return Ok(first.copy_to_bytes(first.remaining()));
    };
    let second = admit_chunk_within_limit(chunk)?;

    let mut bytes = BytesMut::with_capacity(
        first.remaining() + second.remaining() + ADDITIONAL_CAPACITY_FOR_CHUNKS_BEYOND_FIRST_TWO,
    );
    bytes.put(first);
    bytes.put(second);

    while let Some(chunk) = body.next().await {
        bytes.put(admit_chunk_within_limit(chunk)?);
    }

    Ok(bytes.freeze())
}

fn ensure_body_within_limit(
    body: &Bytes,
    encoding: &str,
    max_decompressed_size: usize,
) -> Result<(), ErrorMessage> {
    if body.len() > max_decompressed_size {
        return Err(decompressed_too_large_error(
            encoding,
            max_decompressed_size,
        ));
    }
    Ok(())
}

fn request_body_too_large_error(max: usize) -> ErrorMessage {
    ErrorMessage::new(
        StatusCode::PAYLOAD_TOO_LARGE,
        format!("Request body exceeds limit of {max} bytes."),
    )
}

fn decompressed_too_large_error(encoding: &str, max: usize) -> ErrorMessage {
    ErrorMessage::new(
        StatusCode::PAYLOAD_TOO_LARGE,
        format!("Decompressed {encoding} body exceeds limit of {max} bytes."),
    )
}

/// Maps a decompression failure to a response. If `error` is a [`DecompressedSizeLimitExceeded`]
/// (the decompressed output exceeded the configured size cap), it becomes a `413 Payload Too
/// Large` reporting `max_decompressed_size` (the cap that was actually enforced), matching the
/// request-body and snappy size errors. Any other decode failure emits an `HttpDecompressError`
/// event and becomes a `422 Unprocessable Entity`.
///
/// Callers whose error is not already an [`std::io::Error`] (e.g. snappy) wrap it via
/// [`std::io::Error::other`].
///
/// [`DecompressedSizeLimitExceeded`]: crate::sources::util::decompression::DecompressedSizeLimitExceeded
pub fn emit_decompress_error(
    encoding: &str,
    error: std::io::Error,
    max_decompressed_size: usize,
) -> ErrorMessage {
    if is_decompressed_size_limit_error(&error) {
        return decompressed_too_large_error(encoding, max_decompressed_size);
    }
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
    use futures_util::stream;
    use zstd::stream::Encoder as ZstdEncoder;

    use super::*;

    fn gzip_payload(plaintext: &[u8]) -> Bytes {
        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(plaintext).unwrap();
        encoder.finish().unwrap().into()
    }

    fn zstd_payload_with_window_log(plaintext: &[u8], window_log: u32) -> Bytes {
        let mut encoder = ZstdEncoder::new(Vec::new(), 0).unwrap();
        encoder.window_log(window_log).unwrap();
        encoder.write_all(plaintext).unwrap();
        encoder.finish().unwrap().into()
    }

    #[test]
    fn gzip_within_limit_succeeds() {
        let plaintext = vec![0u8; 10_000];
        let body = gzip_payload(&plaintext);

        let decoded = decompress_body_with_limit(Some("gzip"), body, 100_000).unwrap();
        assert_eq!(decoded.len(), plaintext.len());
    }

    #[test]
    fn gzip_exceeding_limit_returns_413() {
        // Compress 1 MB of zeros, then cap at 1 KB.
        let plaintext = vec![0u8; 1_000_000];
        let body = gzip_payload(&plaintext);

        let err = decompress_body_with_limit(Some("gzip"), body, 1024).expect_err("should reject");
        assert_eq!(err.status_code(), StatusCode::PAYLOAD_TOO_LARGE);
    }

    #[test]
    fn snappy_exceeding_limit_returns_413_before_allocating() {
        // 2 MB of zeros. Snappy keeps the embedded length in the frame header.
        let plaintext = vec![0u8; 2 * 1024 * 1024];
        let compressed = snap::raw::Encoder::new().compress_vec(&plaintext).unwrap();

        let err = decompress_body_with_limit(Some("snappy"), compressed.into(), 1024)
            .expect_err("should reject");
        assert_eq!(err.status_code(), StatusCode::PAYLOAD_TOO_LARGE);
    }

    #[test]
    fn zstd_exceeding_limit_returns_413() {
        let plaintext = vec![0u8; 10_000];
        let compressed = zstd_payload_with_window_log(plaintext.as_slice(), 10);

        let err =
            decompress_body_with_limit(Some("zstd"), compressed, 1024).expect_err("should reject");
        assert_eq!(err.status_code(), StatusCode::PAYLOAD_TOO_LARGE);
    }

    #[test]
    fn identity_passes_through() {
        let body: Bytes = Bytes::from_static(b"hello world");
        let decoded = decompress_body(Some("identity"), body.clone()).unwrap();
        assert_eq!(decoded, body);
    }

    #[test]
    fn identity_exceeding_limit_returns_413() {
        let body = Bytes::from_static(b"hello world");

        let err = decompress_body_with_limit(Some("identity"), body, 5).expect_err("should reject");
        assert_eq!(err.status_code(), StatusCode::PAYLOAD_TOO_LARGE);
    }

    #[test]
    fn missing_content_encoding_exceeding_limit_returns_413() {
        let body = Bytes::from_static(b"hello world");

        let err = decompress_body_with_limit(None, body, 5).expect_err("should reject");
        assert_eq!(err.status_code(), StatusCode::PAYLOAD_TOO_LARGE);
    }

    #[test]
    fn zstd_window_log_tracks_limit() {
        use crate::sources::util::decompression::zstd_window_log_max;
        // Protocol-neutral: the window tracks the decompressed cap (used by gRPC/OTLP, which
        // RFC 9659 does not govern).
        // A zero cap maps to the minimum window log so the allocation guard is never disabled.
        assert_eq!(zstd_window_log_max(0), Some(10));
        assert_eq!(zstd_window_log_max(1), Some(10));
        assert_eq!(zstd_window_log_max(1024), Some(10));
        assert_eq!(zstd_window_log_max(1025), Some(11));
        assert_eq!(
            zstd_window_log_max(DEFAULT_MAX_DECOMPRESSED_SIZE_BYTES),
            Some(27)
        );
    }

    #[test]
    fn http_zstd_window_log_clamps_to_rfc9659_ceiling() {
        use crate::sources::util::decompression::http_zstd_window_log_max;
        // Below the 8 MB (2^23) ceiling the HTTP window still tracks the cap.
        assert_eq!(http_zstd_window_log_max(1024), Some(10));
        assert_eq!(http_zstd_window_log_max(8 * 1024 * 1024), Some(23));
        // At or above the ceiling it clamps to 2^23 instead of tracking the cap, per RFC 9659.
        assert_eq!(http_zstd_window_log_max(16 * 1024 * 1024), Some(23));
        assert_eq!(
            http_zstd_window_log_max(DEFAULT_MAX_DECOMPRESSED_SIZE_BYTES),
            Some(23)
        );
    }

    #[tokio::test]
    async fn collect_body_with_limit_succeeds_within_limit() {
        let body = stream::iter([
            Ok::<_, warp::Error>(Bytes::from_static(b"hello")),
            Ok::<_, warp::Error>(Bytes::from_static(b" world")),
        ]);

        let collected = collect_body_with_limit(body, 11).await.unwrap();
        assert_eq!(collected, Bytes::from_static(b"hello world"));
    }

    #[tokio::test]
    async fn collect_body_with_limit_rejects_oversized_stream() {
        let body = stream::iter([
            Ok::<_, warp::Error>(Bytes::from_static(b"hello")),
            Ok::<_, warp::Error>(Bytes::from_static(b" world")),
        ]);

        let err = collect_body_with_limit(body, 5)
            .await
            .expect_err("should reject");
        assert_eq!(err.status_code(), StatusCode::PAYLOAD_TOO_LARGE);
    }
}
