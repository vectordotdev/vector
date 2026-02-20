use bytes::BytesMut;
use futures::Stream;
use std::{
    io,
    pin::Pin,
    task::{Context, Poll},
};
use tokio::io::AsyncRead;
use tokio_util::codec::{Decoder, FramedRead};

/// Internal wrapper that converts decoder errors into successful results.
///
/// This wrapper transforms a decoder's error result from `Err(error)` into
/// `Ok(Some(Err(error)))`, which prevents `FramedRead` from terminating the stream
/// while still propagating the error to the caller.
struct DecoderResultWrapper<D> {
    inner: D,
}

impl<D> DecoderResultWrapper<D>
where
    D: Decoder,
{
    const fn new(inner: D) -> Self {
        Self { inner }
    }
}

impl<D> Decoder for DecoderResultWrapper<D>
where
    D: Decoder,
{
    type Item = Result<D::Item, D::Error>;
    type Error = io::Error;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        match self.inner.decode(src) {
            Ok(item) => Ok(item.map(Ok)),
            Err(error) => Ok(Some(Err(error))),
        }
    }

    fn decode_eof(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        match self.inner.decode_eof(src) {
            Ok(item) => Ok(item.map(Ok)),
            Err(error) => Ok(Some(Err(error))),
        }
    }
}

/// A `tokio_util::codec::FramedRead` wrapper that continues decoding after recoverable decoder errors.
///
/// # Problem
///
/// The standard `tokio_util::codec::FramedRead` terminates the stream when a decoder
/// returns an error. This is problematic for Vector because:
/// - Vector decoders classify some errors as recoverable (e.g., malformed JSON in one line
///   shouldn't stop processing subsequent valid lines)
/// - Sources need to continue processing data even after encountering decode errors
/// - Metrics and observability require tracking both successful and failed decode attempts
///
/// # Solution
///
/// `DecoderFramedRead` wraps the decoder in a `DecoderResultWrapper` that transforms
/// decoder errors into successful results containing the error. This allows:
/// - The stream to continue after errors
/// - Callers to inspect errors and decide whether to continue (via `StreamDecodingError::can_continue()`)
/// - Proper error metrics and logging
///
/// # When to Use
///
/// Use `DecoderFramedRead` when:
/// - You're using a Vector `Decoder` that implements error recovery logic
/// - You need to continue processing after decode errors
/// - You're processing line-delimited or record-based formats where one bad record shouldn't stop processing
///
/// Use standard `FramedRead` when:
/// - You're using simple decoders (e.g., `CharacterDelimitedDecoder`) that don't need error recovery
/// - Any decode error should terminate the stream
/// - You're working with binary protocols where errors indicate corruption
///
/// # Example
///
/// ```ignore
/// use vector_lib::codecs::{DecoderFramedRead, Decoder};
/// use futures::StreamExt;
///
/// let decoder = Decoder::new(
///     Framer::NewlineDelimited(NewlineDelimitedDecoder::new()),
///     Deserializer::Json(JsonDeserializer::default()),
/// );
///
/// let mut stream = DecoderFramedRead::new(reader, decoder);
///
/// while let Some(result) = stream.next().await {
///     match result {
///         Ok(events) => process_events(events),
///         Err(error) if error.can_continue() => {
///             // Log the error but continue processing
///             warn!("Decode error (continuing): {}", error);
///         }
///         Err(error) => {
///             // Fatal error, stop processing
///             error!("Fatal decode error: {}", error);
///             break;
///         }
///     }
/// }
/// ```
pub struct DecoderFramedRead<T, D> {
    inner: FramedRead<T, DecoderResultWrapper<D>>,
}

impl<T, D> DecoderFramedRead<T, D>
where
    T: AsyncRead,
    D: Decoder,
{
    /// Creates a new `DecoderFramedRead` with the given decoder.
    ///
    /// This wraps the provided decoder to enable error recovery, allowing the stream
    /// to continue processing after recoverable decode errors.
    ///
    /// # Arguments
    ///
    /// * `inner` - The async reader to read from
    /// * `decoder` - The decoder to use for parsing data
    pub fn new(inner: T, decoder: D) -> Self {
        Self {
            inner: FramedRead::new(inner, DecoderResultWrapper::new(decoder)),
        }
    }

    /// Creates a new `DecoderFramedRead` with a specific buffer capacity.
    ///
    /// Use this when you know the expected message size to optimize memory usage.
    ///
    /// # Arguments
    ///
    /// * `inner` - The async reader to read from
    /// * `decoder` - The decoder to use for parsing data
    /// * `capacity` - The initial buffer capacity in bytes
    pub fn with_capacity(inner: T, decoder: D, capacity: usize) -> Self {
        Self {
            inner: FramedRead::with_capacity(inner, DecoderResultWrapper::new(decoder), capacity),
        }
    }

    /// Returns a reference to the underlying I/O stream.
    ///
    /// This is useful for accessing the underlying reader's properties or state
    /// without consuming the `DecoderFramedRead`.
    pub fn get_ref(&self) -> &T {
        self.inner.get_ref()
    }

    /// Returns a mutable reference to the underlying I/O stream.
    ///
    /// This allows modifying the underlying reader's state, though care should be
    /// taken not to interfere with ongoing decoding operations.
    pub fn get_mut(&mut self) -> &mut T {
        self.inner.get_mut()
    }

    /// Returns a reference to the internal read buffer.
    ///
    /// This provides access to any buffered but not yet decoded data. Useful for
    /// debugging or implementing custom recovery logic.
    pub fn read_buffer(&self) -> &BytesMut {
        self.inner.read_buffer()
    }
}

impl<T, D> Stream for DecoderFramedRead<T, D>
where
    T: AsyncRead,
    D: Decoder,
    D::Error: From<io::Error>,
{
    type Item = Result<D::Item, D::Error>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        // SAFETY: This is a pin projection from `self` to the `inner` field.
        // It's safe because:
        // 1. `inner` is not moved after the projection (we only call methods on the pinned reference)
        // 2. The destructor of `DecoderFramedRead` doesn't move `inner`
        // 3. We maintain the pinning invariant - if `self` is pinned, `inner` is pinned
        // 4. `DecoderFramedRead` is not `#[repr(packed)]`
        //
        // TODO: Consider using the `pin-project` crate to automate this safe pin projection
        let inner = unsafe { self.map_unchecked_mut(|this| &mut this.inner) };

        // The DecoderResultWrapper transforms errors into Ok(Err(...)) so the stream continues.
        // We need to unwrap this double Result structure here.
        match inner.poll_next(cx) {
            Poll::Ready(Some(Ok(Ok(item)))) => Poll::Ready(Some(Ok(item))),
            Poll::Ready(Some(Ok(Err(error)))) => Poll::Ready(Some(Err(error))),
            Poll::Ready(Some(Err(error))) => Poll::Ready(Some(Err(error.into()))),
            Poll::Ready(None) => Poll::Ready(None),
            Poll::Pending => Poll::Pending,
        }
    }
}
