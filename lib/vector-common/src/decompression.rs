//! Shared decompression limits used to prevent decompression-bomb (`DoS`) attacks.
//!
//! A length or compressed payload read from an untrusted peer must never drive an unbounded
//! in-memory allocation. This module owns the global decompressed-size cap and the helpers that
//! enforce it, so every source and codec that decompresses untrusted input shares a single,
//! consistently-configured limit.
//!
//! # Usage
//!
//! Wrap any decompression at an untrusted boundary with the appropriate [`CappedDecoder`]
//! constructor and call [`CappedDecoder::decompress`]:
//!
//! ```rust,ignore
//! let data = CappedDecoder::gzip(reader).decompress()?;
//! let data = CappedDecoder::zlib(reader).decompress()?;
//! let data = CappedDecoder::zstd(reader)?.decompress()?;
//! ```
//!
//! The constructors enforce the global decompressed-size cap so that a compression bomb cannot
//! drive unbounded allocation.

// Raw decoder types (flate2 / zstd) are only allowed in this module, which wraps them safely.
#![expect(
    clippy::disallowed_types,
    reason = "this module implements CappedDecoder, the safe wrapper around raw decoders; raw types may only appear here"
)]

use std::{
    fmt,
    io::{self, Read},
    sync::OnceLock,
};

use flate2::read::{MultiGzDecoder, ZlibDecoder};

/// Default cap on the size of any decompressed payload.
///
/// Prevents a compressed "bomb" from causing unbounded memory growth.
pub const DEFAULT_MAX_DECOMPRESSED_SIZE_BYTES: usize = 100 * 1024 * 1024;

static MAX_DECOMPRESSED_SIZE_BYTES: OnceLock<usize> = OnceLock::new();
static MAX_ZLIB_COMPRESSED_FRAME_SIZE_BYTES: OnceLock<usize> = OnceLock::new();
static MAX_ZSTD_WINDOW_LOG: OnceLock<Option<u32>> = OnceLock::new();

/// Maps a decompressed cap to the largest compressed frame that can legitimately produce output
/// within it, using zlib's worst-case expansion of 13.5% + 11 bytes. This lets us reject an
/// oversized declared payload before buffering it, without rejecting a valid frame whose
/// decompressed content stays within the decompressed cap.
///
/// See <https://zlib.net/zlib_tech.html> ("the worst case ... can result in an expansion of at
/// most 13.5%, plus eleven bytes").
#[allow(clippy::cast_possible_truncation)] // limit derives from a usize; saturating math keeps it in range
const fn zlib_compressed_frame_limit(decompressed_limit: usize) -> usize {
    (decompressed_limit as u64)
        .saturating_mul(1135)
        .saturating_div(1000)
        .saturating_add(11) as usize
}

const DEFAULT_MAX_ZLIB_COMPRESSED_FRAME_SIZE_BYTES: usize =
    zlib_compressed_frame_limit(DEFAULT_MAX_DECOMPRESSED_SIZE_BYTES);

const DEFAULT_MAX_ZSTD_WINDOW_LOG: Option<u32> =
    zstd_window_log_max(DEFAULT_MAX_DECOMPRESSED_SIZE_BYTES);

/// Override the global decompressed payload size cap. Must be called before any sources start.
///
/// # Panics
///
/// Panics if called more than once, as the global cap may only be initialized a single time.
pub fn set_max_decompressed_size_bytes(size: usize) {
    MAX_DECOMPRESSED_SIZE_BYTES
        .set(size)
        .expect("max_decompressed_size_bytes already set");
    MAX_ZLIB_COMPRESSED_FRAME_SIZE_BYTES
        .set(zlib_compressed_frame_limit(size))
        .expect("max_zlib_compressed_frame_size_bytes already set");
    MAX_ZSTD_WINDOW_LOG
        .set(zstd_window_log_max(size))
        .expect("max_zstd_window_log already set");
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

/// Smallest zstd `window_log_max` capable of representing `max_decompressed_size` bytes.
///
/// zstd frames declare a window size that the decoder must allocate up front; a crafted frame can
/// request a multi-gigabyte window even though its output would later trip the decompressed-size
/// cap. Clamping the decoder's `window_log_max` to the smallest power-of-two window that can still
/// hold a legitimate payload bounds that allocation. A zero cap maps to the minimum window log
/// (not `None`) so the guard stays at its strictest rather than being disabled.
///
/// This is protocol-neutral: the ceiling is derived from the decompressed cap so any transport's
/// frames decode as long as their window fits the cap. Transports that impose a tighter,
/// spec-mandated window (HTTP `Content-Encoding: zstd`, see [`http_zstd_window_log_max`]) apply
/// that on top.
#[must_use]
#[allow(clippy::manual_clamp)] // `usize::clamp` is not a const fn; the manual form keeps this const
pub const fn zstd_window_log_max(max_decompressed_size: usize) -> Option<u32> {
    const MIN_ZSTD_WINDOW_LOG: u32 = 10;
    const MAX_ZSTD_WINDOW_LOG: u32 = 31;

    // `window_log_max` is expressed as a power-of-two log. Use the smallest zstd window capable of
    // representing the configured byte budget.
    match max_decompressed_size.checked_sub(1) {
        // A zero cap has no representable window; fall back to the smallest window rather than
        // leaving the allocation guard unset.
        None => Some(MIN_ZSTD_WINDOW_LOG),
        Some(max_index) => {
            let window_log = usize::BITS - max_index.leading_zeros();
            let clamped = if window_log < MIN_ZSTD_WINDOW_LOG {
                MIN_ZSTD_WINDOW_LOG
            } else if window_log > MAX_ZSTD_WINDOW_LOG {
                MAX_ZSTD_WINDOW_LOG
            } else {
                window_log
            };
            Some(clamped)
        }
    }
}

/// RFC 9659 window ceiling for zstd under HTTP `Content-Encoding: zstd`: conformant senders require
/// a `Window_Size` of at most 8 MB (2^23) and decoders need only support up to that. This bounds
/// the decoder's window allocation to 8 MB regardless of the (much larger) decompressed cap. It
/// governs HTTP content coding only; other transports (e.g. gRPC/OTLP, whose clients are not bound
/// by RFC 9659 and may legitimately use larger windows) are not clamped to it.
/// See <https://www.rfc-editor.org/info/rfc9659/>.
pub const HTTP_ZSTD_WINDOW_LOG_MAX: u32 = 23;

/// Like [`zstd_window_log_max`] but additionally clamped to the RFC 9659 HTTP window ceiling
/// ([`HTTP_ZSTD_WINDOW_LOG_MAX`]). Use for HTTP `Content-Encoding: zstd`; use the protocol-neutral
/// [`zstd_window_log_max`] for transports RFC 9659 does not govern.
#[must_use]
pub fn http_zstd_window_log_max(max_decompressed_size: usize) -> Option<u32> {
    zstd_window_log_max(max_decompressed_size).map(|w| w.min(HTTP_ZSTD_WINDOW_LOG_MAX))
}

/// Returns the zstd `window_log_max` derived from the global decompressed cap
/// ([`max_decompressed_size_bytes`]).
///
/// Convenience getter for the common case where the decoder window should track the global cap;
/// use [`zstd_window_log_max`] directly when enforcing an explicit, non-global limit (e.g. the
/// HTTP body decompressor's per-call limit).
#[must_use]
pub fn max_zstd_window_log() -> Option<u32> {
    MAX_ZSTD_WINDOW_LOG
        .get()
        .copied()
        .unwrap_or(DEFAULT_MAX_ZSTD_WINDOW_LOG)
}

/// Error raised when a decompressed payload would exceed the configured size cap.
///
/// Surfaced (wrapped in [`io::Error`]) by [`CappedDecoder::decompress`] and the [`CappedReader`]
/// returned by [`CappedDecoder::into_reader`]. Use [`is_decompressed_size_limit_error`] to detect
/// it and distinguish an oversized-input fault from an unrelated I/O error.
#[derive(Debug)]
pub struct DecompressedSizeLimitExceeded;

impl fmt::Display for DecompressedSizeLimitExceeded {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("decompressed size exceeds the configured limit")
    }
}

impl std::error::Error for DecompressedSizeLimitExceeded {}

/// Returns whether `error` was raised because decompression hit the size cap (see
/// [`DecompressedSizeLimitExceeded`]).
#[must_use]
pub fn is_decompressed_size_limit_error(error: &io::Error) -> bool {
    fn is_marker(source: &(dyn std::error::Error + Send + Sync + 'static)) -> bool {
        source.is::<DecompressedSizeLimitExceeded>()
    }

    error.get_ref().is_some_and(is_marker)
}

/// A size-capped decompression reader.
///
/// Wraps any `R: Read` (typically a raw decoder like `MultiGzDecoder` or `ZlibDecoder`) and
/// enforces the configured decompressed-size cap so that a compression bomb cannot drive
/// unbounded memory allocation.
///
/// Construct via the typed class methods ([`CappedDecoder::gzip`], [`CappedDecoder::zlib`],
/// [`CappedDecoder::zstd`]) rather than by wrapping a raw decoder directly. Read the whole payload
/// into memory with [`CappedDecoder::decompress`], or stream it through [`CappedDecoder::into_reader`].
pub struct CappedDecoder<R: Read> {
    inner: io::Take<R>,
    limit: usize,
}

impl<R: Read> CappedDecoder<R> {
    fn with_limit(reader: R, limit: usize) -> Self {
        Self {
            inner: reader.take((limit as u64).saturating_add(1)),
            limit,
        }
    }

    /// Reads all decompressed bytes into a `Vec`, returning an error if the output exceeds the
    /// configured cap.
    ///
    /// # Errors
    ///
    /// Returns an error if reading from the underlying decoder fails, or
    /// [`DecompressedSizeLimitExceeded`] if the decompressed output exceeds the cap.
    pub fn decompress(self) -> io::Result<Vec<u8>> {
        let mut buf = Vec::new();
        self.into_reader().read_to_end(&mut buf)?;
        Ok(buf)
    }

    /// Converts the decoder into a streaming [`CappedReader`] that enforces the cap as bytes are
    /// read, rather than buffering the whole payload up front.
    ///
    /// Prefer this over consuming a raw decoder directly: the returned reader errors out (instead
    /// of silently truncating) the moment the decompressed output would exceed the cap, so a
    /// streaming consumer such as [`io::copy`], `serde_json::from_reader`, or `BufReader` cannot
    /// process a truncated-but-valid-looking payload.
    pub fn into_reader(self) -> CappedReader<R> {
        CappedReader {
            inner: self.inner,
            limit: self.limit,
            consumed: 0,
        }
    }
}

/// A streaming, size-capped decompression reader returned by [`CappedDecoder::into_reader`].
///
/// Yields decompressed bytes incrementally and returns a [`DecompressedSizeLimitExceeded`] error
/// (wrapped in [`io::Error`]) as soon as the cumulative output would exceed the cap.
pub struct CappedReader<R: Read> {
    inner: io::Take<R>,
    limit: usize,
    consumed: usize,
}

impl<R: Read> Read for CappedReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        // The underlying reader is bounded one byte past the cap, so reading beyond `limit` is the
        // unambiguous signal that the payload is oversized.
        let n = self.inner.read(buf)?;
        self.consumed = self.consumed.saturating_add(n);
        if self.consumed > self.limit {
            return Err(io::Error::other(DecompressedSizeLimitExceeded));
        }
        Ok(n)
    }
}

impl<S: Read> CappedDecoder<MultiGzDecoder<S>> {
    /// Creates a capped gzip decoder using the global decompressed-size cap.
    pub fn gzip(reader: S) -> Self {
        Self::gzip_with_limit(reader, max_decompressed_size_bytes())
    }

    /// Creates a capped gzip decoder using an explicit decompressed-size cap.
    pub fn gzip_with_limit(reader: S, limit: usize) -> Self {
        Self::with_limit(MultiGzDecoder::new(reader), limit)
    }
}

impl<S: Read> CappedDecoder<ZlibDecoder<S>> {
    /// Creates a capped zlib/deflate decoder using the global decompressed-size cap.
    pub fn zlib(reader: S) -> Self {
        Self::zlib_with_limit(reader, max_decompressed_size_bytes())
    }

    /// Creates a capped zlib/deflate decoder using an explicit decompressed-size cap.
    pub fn zlib_with_limit(reader: S, limit: usize) -> Self {
        Self::with_limit(ZlibDecoder::new(reader), limit)
    }
}

impl<S: Read> CappedDecoder<zstd::stream::read::Decoder<'static, io::BufReader<S>>> {
    /// Creates a capped zstd decoder using the global decompressed-size cap.
    ///
    /// Also constrains the decoder's internal window allocation via `window_log_max` so a crafted
    /// frame cannot request a large window before the decompressed-size cap trips. The window is
    /// derived from the cap only ([`zstd_window_log_max`]); for HTTP `Content-Encoding: zstd` use
    /// [`zstd_http`](Self::zstd_http), which applies the tighter RFC 9659 ceiling.
    ///
    /// # Errors
    ///
    /// Returns an error if the zstd decoder cannot be initialized (e.g. invalid header).
    pub fn zstd(reader: S) -> io::Result<Self> {
        Self::zstd_with_limit(reader, max_decompressed_size_bytes())
    }

    /// Creates a capped zstd decoder using an explicit decompressed-size cap, with the window
    /// derived from that cap only ([`zstd_window_log_max`]).
    ///
    /// # Errors
    ///
    /// Returns an error if the zstd decoder cannot be initialized (e.g. invalid header).
    pub fn zstd_with_limit(reader: S, limit: usize) -> io::Result<Self> {
        Self::zstd_with_window_log(reader, limit, zstd_window_log_max(limit))
    }

    /// Creates a capped zstd decoder for HTTP `Content-Encoding: zstd` using the global
    /// decompressed-size cap, clamping the decoder window to the RFC 9659 8 MB ceiling
    /// ([`http_zstd_window_log_max`]).
    ///
    /// # Errors
    ///
    /// Returns an error if the zstd decoder cannot be initialized (e.g. invalid header).
    pub fn zstd_http(reader: S) -> io::Result<Self> {
        Self::zstd_http_with_limit(reader, max_decompressed_size_bytes())
    }

    /// Creates a capped zstd decoder for HTTP `Content-Encoding: zstd` using an explicit
    /// decompressed-size cap, clamping the decoder window to the RFC 9659 8 MB ceiling
    /// ([`http_zstd_window_log_max`]).
    ///
    /// # Errors
    ///
    /// Returns an error if the zstd decoder cannot be initialized (e.g. invalid header).
    pub fn zstd_http_with_limit(reader: S, limit: usize) -> io::Result<Self> {
        Self::zstd_with_window_log(reader, limit, http_zstd_window_log_max(limit))
    }

    fn zstd_with_window_log(
        reader: S,
        limit: usize,
        window_log_max: Option<u32>,
    ) -> io::Result<Self> {
        let mut decoder = zstd::stream::read::Decoder::new(reader)?;
        if let Some(window_log_max) = window_log_max {
            decoder.window_log_max(window_log_max)?;
        }
        Ok(Self::with_limit(decoder, limit))
    }
}
