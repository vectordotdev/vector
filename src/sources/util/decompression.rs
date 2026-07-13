use std::sync::OnceLock;

/// Default cap on the size of any decompressed payload.
///
/// Prevents a compressed "bomb" from causing unbounded memory growth.
pub const DEFAULT_MAX_DECOMPRESSED_SIZE_BYTES: usize = 100 * 1024 * 1024;

static MAX_DECOMPRESSED_SIZE_BYTES: OnceLock<usize> = OnceLock::new();
static MAX_ZLIB_COMPRESSED_FRAME_SIZE_BYTES: OnceLock<usize> = OnceLock::new();

/// Maps a decompressed cap to the largest compressed frame that can legitimately produce output
/// within it, using zlib's worst-case expansion of 13.5% + 11 bytes. This lets us reject an
/// oversized declared payload before buffering it, without rejecting a valid frame whose
/// decompressed content stays within the decompressed cap.
///
/// See <https://zlib.net/zlib_tech.html> ("the worst case ... can result in an expansion of at
/// most 13.5%, plus eleven bytes").
const fn zlib_compressed_frame_limit(decompressed_limit: usize) -> usize {
    (decompressed_limit as u64)
        .saturating_mul(1135)
        .saturating_div(1000)
        .saturating_add(11) as usize
}

const DEFAULT_MAX_ZLIB_COMPRESSED_FRAME_SIZE_BYTES: usize =
    zlib_compressed_frame_limit(DEFAULT_MAX_DECOMPRESSED_SIZE_BYTES);

/// Override the global decompressed payload size cap. Must be called before any sources start.
pub fn set_max_decompressed_size_bytes(size: usize) {
    MAX_DECOMPRESSED_SIZE_BYTES
        .set(size)
        .expect("max_decompressed_size_bytes already set");
    MAX_ZLIB_COMPRESSED_FRAME_SIZE_BYTES
        .set(zlib_compressed_frame_limit(size))
        .expect("max_zlib_compressed_frame_size_bytes already set");
}

/// Returns the currently configured decompressed payload size cap.
pub fn max_decompressed_size_bytes() -> usize {
    *MAX_DECOMPRESSED_SIZE_BYTES
        .get()
        .unwrap_or(&DEFAULT_MAX_DECOMPRESSED_SIZE_BYTES)
}

/// Returns the maximum compressed frame wire size we are willing to buffer, derived from the
/// decompressed cap plus zlib's worst-case expansion. See `zlib_compressed_frame_limit`.
pub fn max_zlib_compressed_frame_size_bytes() -> usize {
    *MAX_ZLIB_COMPRESSED_FRAME_SIZE_BYTES
        .get()
        .unwrap_or(&DEFAULT_MAX_ZLIB_COMPRESSED_FRAME_SIZE_BYTES)
}
