pub const GZIP_MAGIC: &[u8] = &[0x1f, 0x8b];
pub const ZLIB_MAGIC: &[u8] = &[0x78];
pub const ZSTD_MAGIC: &[u8] = &[0x28, 0xB5, 0x2F, 0xFD];

/// Maximum size of a zlib stored (uncompressed) block in bytes.
/// See: <https://www.zlib.net/zlib_tech.html>
pub const ZLIB_STORED_BLOCK_SIZE: usize = 16384;

/// Per-block overhead for zlib stored blocks: 1-byte header + 2-byte length + 2-byte ~length.
/// See: <https://www.zlib.net/zlib_tech.html>
pub const ZLIB_STORED_BLOCK_OVERHEAD: usize = 5;

/// Zlib frame overhead: 2-byte header + 4-byte Adler-32 checksum trailer.
/// See: <https://www.zlib.net/zlib_tech.html>
pub const ZLIB_FRAME_OVERHEAD: usize = 6;

/// Threshold below which zstd's `ZSTD_compressBound` adds extra margin (128 KiB).
/// See: <https://github.com/facebook/zstd/blob/dev/lib/zstd.h> (`ZSTD_compressBound`)
pub const ZSTD_SMALL_INPUT_THRESHOLD: usize = 128 << 10;
