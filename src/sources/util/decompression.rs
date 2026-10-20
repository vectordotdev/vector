//! Re-export of the shared decompression limits.
//!
//! The implementation lives in [`vector_common::decompression`] so that both the source crate and
//! `lib/codecs` can enforce the same global decompressed-size cap without duplicating it. This
//! module preserves the historical `crate::sources::util::decompression` import path.
pub use vector_common::decompression::{
    CappedDecoder, CappedReader, DEFAULT_MAX_DECOMPRESSED_SIZE_BYTES,
    DecompressedSizeLimitExceeded, HTTP_ZSTD_WINDOW_LOG_MAX, http_zstd_window_log_max,
    is_decompressed_size_limit_error, max_decompressed_size_bytes,
    max_zlib_compressed_frame_size_bytes, max_zstd_window_log, set_max_decompressed_size_bytes,
    zstd_window_log_max,
};
