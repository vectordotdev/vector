use super::{BoxedFramingError, FramingError};
use crate::{BytesDecoder, StreamDecodingError};
use bytes::{Buf, Bytes, BytesMut};
use derivative::Derivative;
use flate2::read::{MultiGzDecoder, ZlibDecoder};
use snafu::{ensure, ResultExt, Snafu};
use std::any::Any;
use std::collections::HashMap;
use std::io::Read;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio;
use tokio::task::JoinHandle;
use tokio_util::codec::Decoder;
use tracing::{debug, trace, warn};
use vector_common::constants::{GZIP_MAGIC, ZLIB_MAGIC};
use vector_config::configurable_component;

const GELF_MAGIC: &[u8] = &[0x1e, 0x0f];
const GELF_MAX_TOTAL_CHUNKS: u8 = 128;
const DEFAULT_TIMEOUT_SECS: f64 = 5.0;

const fn default_timeout_secs() -> f64 {
    DEFAULT_TIMEOUT_SECS
}

/// Config used to build a `ChunkedGelfDecoder`.
#[configurable_component]
#[derive(Debug, Clone, Default)]
pub struct ChunkedGelfDecoderConfig {
    /// Options for the chunked GELF decoder.
    #[serde(default)]
    pub chunked_gelf: ChunkedGelfDecoderOptions,
}

impl ChunkedGelfDecoderConfig {
    /// Build the `ChunkedGelfDecoder` from this configuration.
    pub fn build(&self) -> ChunkedGelfDecoder {
        ChunkedGelfDecoder::new(
            self.chunked_gelf.timeout_secs,
            self.chunked_gelf.pending_messages_limit,
            self.chunked_gelf.max_length,
            self.chunked_gelf.decompression,
        )
    }
}

/// Options for building a `ChunkedGelfDecoder`.
#[configurable_component]
#[derive(Clone, Debug, Derivative)]
#[derivative(Default)]
pub struct ChunkedGelfDecoderOptions {
    /// The timeout, in seconds, for a message to be fully received. If the timeout is reached, the
    /// decoder drops all the received chunks of the timed out message.
    #[serde(default = "default_timeout_secs")]
    #[derivative(Default(value = "default_timeout_secs()"))]
    pub timeout_secs: f64,

    /// The maximum number of pending incomplete messages. If this limit is reached, the decoder starts
    /// dropping chunks of new messages, ensuring the memory usage of the decoder's state is bounded.
    /// If this option is not set, the decoder does not limit the number of pending messages and the memory usage
    /// of its messages buffer can grow unbounded. This matches Graylog Server's behavior.
    #[serde(default, skip_serializing_if = "vector_core::serde::is_default")]
    pub pending_messages_limit: Option<usize>,

    /// The maximum length of a single GELF message, in bytes. Messages longer than this length will
    /// be dropped. If this option is not set, the decoder does not limit the length of messages and
    /// the per-message memory is unbounded.
    ///
    /// **Note**: A message can be composed of multiple chunks and this limit is applied to the whole
    /// message, not to individual chunks.
    ///
    /// This limit takes only into account the message's payload and the GELF header bytes are excluded from the calculation.
    /// The message's payload is the concatenation of all the chunks' payloads.
    #[serde(default, skip_serializing_if = "vector_core::serde::is_default")]
    pub max_length: Option<usize>,

    /// Decompression configuration for GELF messages.
    #[serde(default, skip_serializing_if = "vector_core::serde::is_default")]
    pub decompression: ChunkedGelfDecompressionConfig,
}

/// Decompression options for ChunkedGelfDecoder.
#[configurable_component]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Derivative)]
#[derivative(Default)]
pub enum ChunkedGelfDecompressionConfig {
    /// Automatically detect the decompression method based on the magic bytes of the message.
    #[derivative(Default)]
    Auto,
    /// Use Gzip decompression.
    Gzip,
    /// Use Zlib decompression.
    Zlib,
    /// Do not decompress the message.
    None,
}

impl ChunkedGelfDecompressionConfig {
    pub fn get_decompression(&self, data: &Bytes) -> ChunkedGelfDecompression {
        match self {
            Self::Auto => ChunkedGelfDecompression::from_magic(data),
            Self::Gzip => ChunkedGelfDecompression::Gzip,
            Self::Zlib => ChunkedGelfDecompression::Zlib,
            Self::None => ChunkedGelfDecompression::None,
        }
    }
}

#[derive(Debug)]
struct MessageState {
    total_chunks: u8,
    chunks: [Bytes; GELF_MAX_TOTAL_CHUNKS as usize],
    chunks_bitmap: u128,
    current_length: usize,
    timeout_task: JoinHandle<()>,
}

impl MessageState {
    pub const fn new(total_chunks: u8, timeout_task: JoinHandle<()>) -> Self {
        Self {
            total_chunks,
            chunks: [const { Bytes::new() }; GELF_MAX_TOTAL_CHUNKS as usize],
            chunks_bitmap: 0,
            current_length: 0,
            timeout_task,
        }
    }

    fn is_chunk_present(&self, sequence_number: u8) -> bool {
        let chunk_bitmap_id = 1 << sequence_number;
        self.chunks_bitmap & chunk_bitmap_id != 0
    }

    fn add_chunk(&mut self, sequence_number: u8, chunk: Bytes) {
        let chunk_bitmap_id = 1 << sequence_number;
        self.chunks_bitmap |= chunk_bitmap_id;
        self.current_length += chunk.remaining();
        self.chunks[sequence_number as usize] = chunk;
    }

    fn is_complete(&self) -> bool {
        self.chunks_bitmap.count_ones() == self.total_chunks as u32
    }

    fn current_length(&self) -> usize {
        self.current_length
    }

    fn retrieve_message(&self) -> Option<Bytes> {
        if self.is_complete() {
            self.timeout_task.abort();
            let chunks = &self.chunks[0..self.total_chunks as usize];
            let mut message = BytesMut::new();
            for chunk in chunks {
                message.extend_from_slice(chunk);
            }
            Some(message.freeze())
        } else {
            None
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum ChunkedGelfDecompression {
    Gzip,
    Zlib,
    None,
}

impl ChunkedGelfDecompression {
    pub fn from_magic(data: &Bytes) -> Self {
        if data.starts_with(GZIP_MAGIC) {
            trace!("Detected Gzip compression");
            return Self::Gzip;
        }

        if data.starts_with(ZLIB_MAGIC) {
            // Based on https://datatracker.ietf.org/doc/html/rfc1950#section-2.2
            if let Some([first_byte, second_byte]) = data.get(0..2) {
                if (*first_byte as u16 * 256 + *second_byte as u16) % 31 == 0 {
                    trace!("Detected Zlib compression");
                    return Self::Zlib;
                }
            };

            warn!(
                "Detected Zlib magic bytes but the header is invalid: {:?}",
                data.get(0..2)
            );
        };

        trace!("No compression detected",);
        Self::None
    }

    pub fn decompress(&self, data: Bytes) -> Result<Bytes, ChunkedGelfDecompressionError> {
        let decompressed = match self {
            Self::Gzip => {
                let mut decoder = MultiGzDecoder::new(data.reader());
                let mut decompressed = Vec::new();
                decoder
                    .read_to_end(&mut decompressed)
                    .context(GzipDecompressionSnafu)?;
                Bytes::from(decompressed)
            }
            Self::Zlib => {
                let mut decoder = ZlibDecoder::new(data.reader());
                let mut decompressed = Vec::new();
                decoder
                    .read_to_end(&mut decompressed)
                    .context(ZlibDecompressionSnafu)?;
                Bytes::from(decompressed)
            }
            Self::None => data,
        };
        Ok(decompressed)
    }
}

#[derive(Debug, Snafu)]
pub enum ChunkedGelfDecompressionError {
    #[snafu(display("Gzip decompression error: {source}"))]
    GzipDecompression { source: std::io::Error },
    #[snafu(display("Zlib decompression error: {source}"))]
    ZlibDecompression { source: std::io::Error },
}

#[derive(Debug, Snafu)]
pub enum ChunkedGelfDecoderError {
    #[snafu(display("Invalid chunk header with less than 10 bytes: 0x{header:0x}"))]
    InvalidChunkHeader { header: Bytes },
    #[snafu(display("Received chunk with message id {message_id} and sequence number {sequence_number} has an invalid total chunks value of {total_chunks}. It must be between 1 and {GELF_MAX_TOTAL_CHUNKS}."))]
    InvalidTotalChunks {
        message_id: u64,
        sequence_number: u8,
        total_chunks: u8,
    },
    #[snafu(display("Received chunk with message id {message_id} and sequence number {sequence_number} has a sequence number greater than its total chunks value of {total_chunks}"))]
    InvalidSequenceNumber {
        message_id: u64,
        sequence_number: u8,
        total_chunks: u8,
    },
    #[snafu(display("Pending messages limit of {pending_messages_limit} reached while processing chunk with message id {message_id} and sequence number {sequence_number}"))]
    PendingMessagesLimitReached {
        message_id: u64,
        sequence_number: u8,
        pending_messages_limit: usize,
    },
    #[snafu(display("Received chunk with message id {message_id} and sequence number {sequence_number} has different total chunks values: original total chunks value is {original_total_chunks} and received total chunks value is {received_total_chunks}"))]
    TotalChunksMismatch {
        message_id: u64,
        sequence_number: u8,
        original_total_chunks: u8,
        received_total_chunks: u8,
    },
    #[snafu(display("Message with id {message_id} has exceeded the maximum message length and it will be dropped: got {length} bytes and max message length is {max_length} bytes. Discarding all buffered chunks of that message"))]
    MaxLengthExceed {
        message_id: u64,
        sequence_number: u8,
        length: usize,
        max_length: usize,
    },
    #[snafu(display("Error while decompressing message. {source}"))]
    Decompression {
        source: ChunkedGelfDecompressionError,
    },
}

impl StreamDecodingError for ChunkedGelfDecoderError {
    fn can_continue(&self) -> bool {
        true
    }
}

impl FramingError for ChunkedGelfDecoderError {
    fn as_any(&self) -> &dyn Any {
        self as &dyn Any
    }
}

/// A codec for handling GELF messages that may be chunked. The implementation is based on [Graylog's GELF documentation](https://go2docs.graylog.org/5-0/getting_in_log_data/gelf.html#GELFviaUDP)
/// and [Graylog's go-gelf library](https://github.com/Graylog2/go-gelf/blob/v1/gelf/reader.go).
#[derive(Debug, Clone)]
pub struct ChunkedGelfDecoder {
    // We have to use this decoder to read all the bytes from the buffer first and don't let tokio
    // read it buffered, as tokio FramedRead will not always call the decode method with the
    // whole message. (see https://docs.rs/tokio-util/latest/src/tokio_util/codec/framed_impl.rs.html#26).
    // This limitation is due to the fact that the GELF format does not specify the length of the
    // message, so we have to read all the bytes from the message (datagram)
    bytes_decoder: BytesDecoder,
    decompression_config: ChunkedGelfDecompressionConfig,
    state: Arc<Mutex<HashMap<u64, MessageState>>>,
    timeout: Duration,
    pending_messages_limit: Option<usize>,
    max_length: Option<usize>,
}

impl ChunkedGelfDecoder {
    /// Creates a new `ChunkedGelfDecoder`.
    pub fn new(
        timeout_secs: f64,
        pending_messages_limit: Option<usize>,
        max_length: Option<usize>,
        decompression_config: ChunkedGelfDecompressionConfig,
    ) -> Self {
        Self {
            bytes_decoder: BytesDecoder::new(),
            decompression_config,
            state: Arc::new(Mutex::new(HashMap::new())),
            timeout: Duration::from_secs_f64(timeout_secs),
            pending_messages_limit,
            max_length,
        }
    }

    /// Decode a GELF chunk
    pub fn decode_chunk(
        &mut self,
        mut chunk: Bytes,
    ) -> Result<Option<Bytes>, ChunkedGelfDecoderError> {
        // Encoding scheme:
        //
        // +------------+-----------------+--------------+----------------------+
        // | Message id | Sequence number | Total chunks |    Chunk payload     |
        // +------------+-----------------+--------------+----------------------+
        // | 64 bits    | 8 bits          | 8 bits       | remaining bits       |
        // +------------+-----------------+--------------+----------------------+
        //
        // As this codec is oriented for UDP, the chunks (datagrams) are not guaranteed to be received in order,
        // nor to be received at all. So, we have to store the chunks in a buffer (state field) until we receive
        // all the chunks of a message. When we receive all the chunks of a message, we can concatenate them
        // and return the complete payload.

        // We need 10 bytes to read the message id, sequence number and total chunks
        ensure!(
            chunk.remaining() >= 10,
            InvalidChunkHeaderSnafu { header: chunk }
        );

        let message_id = chunk.get_u64();
        let sequence_number = chunk.get_u8();
        let total_chunks = chunk.get_u8();

        ensure!(
            total_chunks > 0 && total_chunks <= GELF_MAX_TOTAL_CHUNKS,
            InvalidTotalChunksSnafu {
                message_id,
                sequence_number,
                total_chunks
            }
        );

        ensure!(
            sequence_number < total_chunks,
            InvalidSequenceNumberSnafu {
                message_id,
                sequence_number,
                total_chunks
            }
        );

        let mut state_lock = self.state.lock().expect("poisoned lock");

        if let Some(pending_messages_limit) = self.pending_messages_limit {
            ensure!(
                state_lock.len() < pending_messages_limit,
                PendingMessagesLimitReachedSnafu {
                    message_id,
                    sequence_number,
                    pending_messages_limit
                }
            );
        }

        let message_state = state_lock.entry(message_id).or_insert_with(|| {
            // We need to spawn a task that will clear the message state after a certain time
            // otherwise we will have a memory leak due to messages that never complete
            let state = Arc::clone(&self.state);
            let timeout = self.timeout;
            let timeout_handle = tokio::spawn(async move {
                tokio::time::sleep(timeout).await;
                let mut state_lock = state.lock().expect("poisoned lock");
                if state_lock.remove(&message_id).is_some() {
                    warn!(
                        message_id = message_id,
                        timeout_secs = timeout.as_secs_f64(),
                        "Message was not fully received within the timeout window. Discarding it."
                    );
                }
            });
            MessageState::new(total_chunks, timeout_handle)
        });

        ensure!(
            message_state.total_chunks == total_chunks,
            TotalChunksMismatchSnafu {
                message_id,
                sequence_number,
                original_total_chunks: message_state.total_chunks,
                received_total_chunks: total_chunks
            }
        );

        if message_state.is_chunk_present(sequence_number) {
            debug!(
                message_id = message_id,
                sequence_number = sequence_number,
                "Received a duplicate chunk. Ignoring it."
            );
            return Ok(None);
        }

        message_state.add_chunk(sequence_number, chunk);

        if let Some(max_length) = self.max_length {
            let length = message_state.current_length();
            if length > max_length {
                state_lock.remove(&message_id);
                return Err(ChunkedGelfDecoderError::MaxLengthExceed {
                    message_id,
                    sequence_number,
                    length,
                    max_length,
                });
            }
        }

        if let Some(message) = message_state.retrieve_message() {
            state_lock.remove(&message_id);
            Ok(Some(message))
        } else {
            Ok(None)
        }
    }

    /// Decode a GELF message that may be chunked or not. The source bytes are expected to be
    /// datagram-based (or message-based), so it must not contain multiple GELF messages
    /// delimited by '\0', such as it would be in a stream-based protocol.
    pub fn decode_message(
        &mut self,
        mut src: Bytes,
    ) -> Result<Option<Bytes>, ChunkedGelfDecoderError> {
        let message = if src.starts_with(GELF_MAGIC) {
            trace!("Received a chunked GELF message based on the magic bytes");
            src.advance(2);
            self.decode_chunk(src)?
        } else {
            trace!(
                "Received an unchunked GELF message. First two bytes of message: {:?}",
                &src[0..2]
            );
            Some(src)
        };

        // We can have both chunked and unchunked messages that are compressed
        message
            .map(|message| {
                self.decompression_config
                    .get_decompression(&message)
                    .decompress(message)
                    .context(DecompressionSnafu)
            })
            .transpose()
    }
}

impl Default for ChunkedGelfDecoder {
    fn default() -> Self {
        Self::new(
            DEFAULT_TIMEOUT_SECS,
            None,
            None,
            ChunkedGelfDecompressionConfig::Auto,
        )
    }
}

impl Decoder for ChunkedGelfDecoder {
    type Item = Bytes;

    type Error = BoxedFramingError;

    fn decode(&mut self, src: &mut bytes::BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        if src.is_empty() {
            return Ok(None);
        }

        Ok(self
            .bytes_decoder
            .decode(src)?
            .and_then(|frame| self.decode_message(frame).transpose())
            .transpose()?)
    }
    fn decode_eof(&mut self, buf: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        if buf.is_empty() {
            return Ok(None);
        }

        Ok(self
            .bytes_decoder
            .decode_eof(buf)?
            .and_then(|frame| self.decode_message(frame).transpose())
            .transpose()?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::{BufMut, BytesMut};
    use flate2::{write::GzEncoder, write::ZlibEncoder};
    use rand::{rngs::SmallRng, seq::SliceRandom, SeedableRng};
    use rstest::{fixture, rstest};
    use std::fmt::Write as FmtWrite;
    use std::io::Write as IoWrite;
    use tracing_test::traced_test;

    pub enum Compression {
        Gzip,
        Zlib,
    }

    impl Compression {
        pub fn compress(&self, payload: &impl AsRef<[u8]>) -> Bytes {
            self.compress_with_level(payload, flate2::Compression::default())
        }

        pub fn compress_with_level(
            &self,
            payload: &impl AsRef<[u8]>,
            level: flate2::Compression,
        ) -> Bytes {
            match self {
                Compression::Gzip => {
                    let mut encoder = GzEncoder::new(Vec::new(), level);
                    encoder
                        .write_all(payload.as_ref())
                        .expect("failed to write to encoder");
                    encoder.finish().expect("failed to finish encoder").into()
                }
                Compression::Zlib => {
                    let mut encoder = ZlibEncoder::new(Vec::new(), level);
                    encoder
                        .write_all(payload.as_ref())
                        .expect("failed to write to encoder");
                    encoder.finish().expect("failed to finish encoder").into()
                }
            }
        }
    }

    fn create_chunk(
        message_id: u64,
        sequence_number: u8,
        total_chunks: u8,
        payload: &impl AsRef<[u8]>,
    ) -> BytesMut {
        let mut chunk = BytesMut::new();
        chunk.put_slice(GELF_MAGIC);
        chunk.put_u64(message_id);
        chunk.put_u8(sequence_number);
        chunk.put_u8(total_chunks);
        chunk.extend_from_slice(payload.as_ref());
        chunk
    }

    #[fixture]
    fn unchunked_message() -> (BytesMut, String) {
        let payload = "foo";
        (BytesMut::from(payload), payload.to_string())
    }

    #[fixture]
    fn two_chunks_message() -> ([BytesMut; 2], String) {
        let message_id = 1u64;
        let total_chunks = 2u8;

        let first_sequence_number = 0u8;
        let first_payload = "foo";
        let first_chunk = create_chunk(
            message_id,
            first_sequence_number,
            total_chunks,
            &first_payload,
        );

        let second_sequence_number = 1u8;
        let second_payload = "bar";
        let second_chunk = create_chunk(
            message_id,
            second_sequence_number,
            total_chunks,
            &second_payload,
        );

        (
            [first_chunk, second_chunk],
            format!("{first_payload}{second_payload}"),
        )
    }

    #[fixture]
    fn three_chunks_message() -> ([BytesMut; 3], String) {
        let message_id = 2u64;
        let total_chunks = 3u8;

        let first_sequence_number = 0u8;
        let first_payload = "foo";
        let first_chunk = create_chunk(
            message_id,
            first_sequence_number,
            total_chunks,
            &first_payload,
        );

        let second_sequence_number = 1u8;
        let second_payload = "bar";
        let second_chunk = create_chunk(
            message_id,
            second_sequence_number,
            total_chunks,
            &second_payload,
        );

        let third_sequence_number = 2u8;
        let third_payload = "baz";
        let third_chunk = create_chunk(
            message_id,
            third_sequence_number,
            total_chunks,
            &third_payload,
        );

        (
            [first_chunk, second_chunk, third_chunk],
            format!("{first_payload}{second_payload}{third_payload}"),
        )
    }

    fn downcast_framing_error(error: &BoxedFramingError) -> &ChunkedGelfDecoderError {
        error
            .as_any()
            .downcast_ref::<ChunkedGelfDecoderError>()
            .expect("Expected ChunkedGelfDecoderError to be downcasted")
    }

    #[rstest]
    #[tokio::test]
    async fn decode_chunked(two_chunks_message: ([BytesMut; 2], String)) {
        let (mut chunks, expected_message) = two_chunks_message;
        let mut decoder = ChunkedGelfDecoder::default();

        let frame = decoder.decode_eof(&mut chunks[0]).unwrap();
        assert!(frame.is_none());

        let frame = decoder.decode_eof(&mut chunks[1]).unwrap();
        assert_eq!(frame, Some(Bytes::from(expected_message)));
    }

    #[rstest]
    #[tokio::test]
    async fn decode_unchunked(unchunked_message: (BytesMut, String)) {
        let (mut message, expected_message) = unchunked_message;
        let mut decoder = ChunkedGelfDecoder::default();

        let frame = decoder.decode_eof(&mut message).unwrap();
        assert_eq!(frame, Some(Bytes::from(expected_message)));
    }

    #[rstest]
    #[tokio::test]
    async fn decode_unordered_chunks(two_chunks_message: ([BytesMut; 2], String)) {
        let (mut chunks, expected_message) = two_chunks_message;
        let mut decoder = ChunkedGelfDecoder::default();

        let frame = decoder.decode_eof(&mut chunks[1]).unwrap();
        assert!(frame.is_none());

        let frame = decoder.decode_eof(&mut chunks[0]).unwrap();
        assert_eq!(frame, Some(Bytes::from(expected_message)));
    }

    #[rstest]
    #[tokio::test]
    async fn decode_unordered_messages(
        two_chunks_message: ([BytesMut; 2], String),
        three_chunks_message: ([BytesMut; 3], String),
    ) {
        let (mut two_chunks, two_chunks_expected) = two_chunks_message;
        let (mut three_chunks, three_chunks_expected) = three_chunks_message;
        let mut decoder = ChunkedGelfDecoder::default();

        let frame = decoder.decode_eof(&mut three_chunks[2]).unwrap();
        assert!(frame.is_none());

        let frame = decoder.decode_eof(&mut two_chunks[0]).unwrap();
        assert!(frame.is_none());

        let frame = decoder.decode_eof(&mut three_chunks[0]).unwrap();
        assert!(frame.is_none());

        let frame = decoder.decode_eof(&mut two_chunks[1]).unwrap();
        assert_eq!(frame, Some(Bytes::from(two_chunks_expected)));

        let frame = decoder.decode_eof(&mut three_chunks[1]).unwrap();
        assert_eq!(frame, Some(Bytes::from(three_chunks_expected)));
    }

    #[rstest]
    #[tokio::test]
    async fn decode_mixed_chunked_and_unchunked_messages(
        unchunked_message: (BytesMut, String),
        two_chunks_message: ([BytesMut; 2], String),
    ) {
        let (mut unchunked_message, expected_unchunked_message) = unchunked_message;
        let (mut chunks, expected_chunked_message) = two_chunks_message;
        let mut decoder = ChunkedGelfDecoder::default();

        let frame = decoder.decode_eof(&mut chunks[1]).unwrap();
        assert!(frame.is_none());

        let frame = decoder.decode_eof(&mut unchunked_message).unwrap();
        assert_eq!(frame, Some(Bytes::from(expected_unchunked_message)));

        let frame = decoder.decode_eof(&mut chunks[0]).unwrap();
        assert_eq!(frame, Some(Bytes::from(expected_chunked_message)));
    }

    #[tokio::test]
    async fn decode_shuffled_messages() {
        let mut rng = SmallRng::seed_from_u64(42);
        let total_chunks = 100u8;
        let first_message_id = 1u64;
        let first_payload = "first payload";
        let second_message_id = 2u64;
        let second_payload = "second payload";
        let first_message_chunks = (0..total_chunks).map(|sequence_number| {
            create_chunk(
                first_message_id,
                sequence_number,
                total_chunks,
                &first_payload,
            )
        });
        let second_message_chunks = (0..total_chunks).map(|sequence_number| {
            create_chunk(
                second_message_id,
                sequence_number,
                total_chunks,
                &second_payload,
            )
        });
        let expected_first_message = first_payload.repeat(total_chunks as usize);
        let expected_second_message = second_payload.repeat(total_chunks as usize);
        let mut merged_chunks = first_message_chunks
            .chain(second_message_chunks)
            .collect::<Vec<_>>();
        merged_chunks.shuffle(&mut rng);
        let mut decoder = ChunkedGelfDecoder::default();

        let mut count = 0;
        let first_retrieved_message = loop {
            assert!(count < 2 * total_chunks as usize);
            if let Some(message) = decoder.decode_eof(&mut merged_chunks[count]).unwrap() {
                break message;
            } else {
                count += 1;
            }
        };
        let second_retrieved_message = loop {
            assert!(count < 2 * total_chunks as usize);
            if let Some(message) = decoder.decode_eof(&mut merged_chunks[count]).unwrap() {
                break message;
            } else {
                count += 1
            }
        };

        assert_eq!(second_retrieved_message, expected_first_message);
        assert_eq!(first_retrieved_message, expected_second_message);
    }

    #[rstest]
    #[tokio::test(start_paused = true)]
    #[traced_test]
    async fn decode_timeout(two_chunks_message: ([BytesMut; 2], String)) {
        let (mut chunks, _) = two_chunks_message;
        let mut decoder = ChunkedGelfDecoder::default();

        let frame = decoder.decode_eof(&mut chunks[0]).unwrap();
        assert!(frame.is_none());
        assert!(!decoder.state.lock().unwrap().is_empty());

        // The message state should be cleared after a certain time
        tokio::time::sleep(Duration::from_secs_f64(DEFAULT_TIMEOUT_SECS + 1.0)).await;
        assert!(decoder.state.lock().unwrap().is_empty());
        assert!(logs_contain(
            "Message was not fully received within the timeout window. Discarding it."
        ));

        let frame = decoder.decode_eof(&mut chunks[1]).unwrap();
        assert!(frame.is_none());

        tokio::time::sleep(Duration::from_secs_f64(DEFAULT_TIMEOUT_SECS + 1.0)).await;
        assert!(decoder.state.lock().unwrap().is_empty());
        assert!(logs_contain(
            "Message was not fully received within the timeout window. Discarding it"
        ));
    }

    #[tokio::test]
    async fn decode_empty_input() {
        let mut src = BytesMut::new();
        let mut decoder = ChunkedGelfDecoder::default();

        let frame = decoder.decode_eof(&mut src).unwrap();
        assert!(frame.is_none());
    }

    #[tokio::test]
    async fn decode_chunk_with_invalid_header() {
        let mut src = BytesMut::new();
        src.extend_from_slice(GELF_MAGIC);
        // Invalid chunk header with less than 10 bytes
        let invalid_chunk = [0x12, 0x34];
        src.extend_from_slice(&invalid_chunk);
        let mut decoder = ChunkedGelfDecoder::default();
        let frame = decoder.decode_eof(&mut src);

        let error = frame.unwrap_err();
        let downcasted_error = downcast_framing_error(&error);
        assert!(matches!(
            downcasted_error,
            ChunkedGelfDecoderError::InvalidChunkHeader { .. }
        ));
    }

    #[tokio::test]
    async fn decode_chunk_with_invalid_total_chunks() {
        let message_id = 1u64;
        let sequence_number = 1u8;
        let invalid_total_chunks = GELF_MAX_TOTAL_CHUNKS + 1;
        let payload = "foo";
        let mut chunk = create_chunk(message_id, sequence_number, invalid_total_chunks, &payload);
        let mut decoder = ChunkedGelfDecoder::default();

        let frame = decoder.decode_eof(&mut chunk);
        let error = frame.unwrap_err();
        let downcasted_error = downcast_framing_error(&error);
        assert!(matches!(
            downcasted_error,
            ChunkedGelfDecoderError::InvalidTotalChunks {
                message_id: 1,
                sequence_number: 1,
                total_chunks: 129,
            }
        ));
    }

    #[tokio::test]
    async fn decode_chunk_with_invalid_sequence_number() {
        let message_id = 1u64;
        let total_chunks = 2u8;
        let invalid_sequence_number = total_chunks + 1;
        let payload = "foo";
        let mut chunk = create_chunk(message_id, invalid_sequence_number, total_chunks, &payload);
        let mut decoder = ChunkedGelfDecoder::default();

        let frame = decoder.decode_eof(&mut chunk);
        let error = frame.unwrap_err();
        let downcasted_error = downcast_framing_error(&error);
        assert!(matches!(
            downcasted_error,
            ChunkedGelfDecoderError::InvalidSequenceNumber {
                message_id: 1,
                sequence_number: 3,
                total_chunks: 2,
            }
        ));
    }

    #[rstest]
    #[tokio::test]
    async fn decode_reached_pending_messages_limit(
        two_chunks_message: ([BytesMut; 2], String),
        three_chunks_message: ([BytesMut; 3], String),
    ) {
        let (mut two_chunks, _) = two_chunks_message;
        let (mut three_chunks, _) = three_chunks_message;
        let mut decoder = ChunkedGelfDecoder {
            pending_messages_limit: Some(1),
            ..Default::default()
        };

        let frame = decoder.decode_eof(&mut two_chunks[0]).unwrap();
        assert!(frame.is_none());
        assert!(decoder.state.lock().unwrap().len() == 1);

        let frame = decoder.decode_eof(&mut three_chunks[0]);
        let error = frame.unwrap_err();
        let downcasted_error = downcast_framing_error(&error);
        assert!(matches!(
            downcasted_error,
            ChunkedGelfDecoderError::PendingMessagesLimitReached {
                message_id: 2u64,
                sequence_number: 0u8,
                pending_messages_limit: 1,
            }
        ));
        assert!(decoder.state.lock().unwrap().len() == 1);
    }

    #[rstest]
    #[tokio::test]
    async fn decode_chunk_with_different_total_chunks() {
        let message_id = 1u64;
        let sequence_number = 0u8;
        let total_chunks = 2u8;
        let payload = "foo";
        let mut first_chunk = create_chunk(message_id, sequence_number, total_chunks, &payload);
        let mut second_chunk =
            create_chunk(message_id, sequence_number + 1, total_chunks + 1, &payload);
        let mut decoder = ChunkedGelfDecoder::default();

        let frame = decoder.decode_eof(&mut first_chunk).unwrap();
        assert!(frame.is_none());

        let frame = decoder.decode_eof(&mut second_chunk);
        let error = frame.unwrap_err();
        let downcasted_error = downcast_framing_error(&error);
        assert!(matches!(
            downcasted_error,
            ChunkedGelfDecoderError::TotalChunksMismatch {
                message_id: 1,
                sequence_number: 1,
                original_total_chunks: 2,
                received_total_chunks: 3,
            }
        ));
    }

    #[rstest]
    #[tokio::test]
    async fn decode_message_greater_than_max_length(two_chunks_message: ([BytesMut; 2], String)) {
        let (mut chunks, _) = two_chunks_message;
        let mut decoder = ChunkedGelfDecoder {
            max_length: Some(5),
            ..Default::default()
        };

        let frame = decoder.decode_eof(&mut chunks[0]).unwrap();
        assert!(frame.is_none());
        let frame = decoder.decode_eof(&mut chunks[1]);
        let error = frame.unwrap_err();
        let downcasted_error = downcast_framing_error(&error);
        assert!(matches!(
            downcasted_error,
            ChunkedGelfDecoderError::MaxLengthExceed {
                message_id: 1,
                sequence_number: 1,
                length: 6,
                max_length: 5,
            }
        ));
        assert_eq!(decoder.state.lock().unwrap().len(), 0);
    }

    #[rstest]
    #[tokio::test]
    #[traced_test]
    async fn decode_duplicated_chunk(two_chunks_message: ([BytesMut; 2], String)) {
        let (mut chunks, _) = two_chunks_message;
        let mut decoder = ChunkedGelfDecoder::default();

        let frame = decoder.decode_eof(&mut chunks[0].clone()).unwrap();
        assert!(frame.is_none());

        let frame = decoder.decode_eof(&mut chunks[0]).unwrap();
        assert!(frame.is_none());
        assert!(logs_contain("Received a duplicate chunk. Ignoring it."));
    }

    #[tokio::test]
    #[rstest]
    #[case::gzip(Compression::Gzip)]
    #[case::zlib(Compression::Zlib)]
    async fn decode_compressed_unchunked_message(#[case] compression: Compression) {
        let payload = (0..100).fold(String::new(), |mut payload, n| {
            write!(payload, "foo{n}").unwrap();
            payload
        });
        let compressed_payload = compression.compress(&payload);
        let mut decoder = ChunkedGelfDecoder::default();

        let frame = decoder
            .decode_eof(&mut compressed_payload.into())
            .expect("decoding should not fail")
            .expect("decoding should return a frame");

        assert_eq!(frame, payload);
    }

    #[tokio::test]
    #[rstest]
    #[case::gzip(Compression::Gzip)]
    #[case::zlib(Compression::Zlib)]
    async fn decode_compressed_chunked_message(#[case] compression: Compression) {
        let message_id = 1u64;
        let max_chunk_size = 5;
        let payload = (0..100).fold(String::new(), |mut payload, n| {
            write!(payload, "foo{n}").unwrap();
            payload
        });
        let compressed_payload = compression.compress(&payload);
        let total_chunks = compressed_payload.len().div_ceil(max_chunk_size) as u8;
        assert!(total_chunks < GELF_MAX_TOTAL_CHUNKS);
        let mut chunks = compressed_payload
            .chunks(max_chunk_size)
            .enumerate()
            .map(|(i, chunk)| create_chunk(message_id, i as u8, total_chunks, &chunk))
            .collect::<Vec<_>>();
        let (last_chunk, first_chunks) =
            chunks.split_last_mut().expect("chunks should not be empty");
        let mut decoder = ChunkedGelfDecoder::default();

        for chunk in first_chunks {
            let frame = decoder.decode_eof(chunk).expect("decoding should not fail");
            assert!(frame.is_none());
        }
        let frame = decoder
            .decode_eof(last_chunk)
            .expect("decoding should not fail")
            .expect("decoding should return a frame");

        assert_eq!(frame, payload);
    }

    #[tokio::test]
    async fn decode_malformed_gzip_message() {
        let mut compressed_payload = BytesMut::new();
        compressed_payload.extend(GZIP_MAGIC);
        compressed_payload.extend(&[0x12, 0x34, 0x56, 0x78]);
        let mut decoder = ChunkedGelfDecoder::default();

        let error = decoder
            .decode_eof(&mut compressed_payload)
            .expect_err("decoding should fail");

        let downcasted_error = downcast_framing_error(&error);
        assert!(matches!(
            downcasted_error,
            ChunkedGelfDecoderError::Decompression {
                source: ChunkedGelfDecompressionError::GzipDecompression { .. }
            }
        ));
    }

    #[tokio::test]
    async fn decode_malformed_zlib_message() {
        let mut compressed_payload = BytesMut::new();
        compressed_payload.extend(ZLIB_MAGIC);
        compressed_payload.extend(&[0x9c, 0x12, 0x00, 0xFF]);
        let mut decoder = ChunkedGelfDecoder::default();

        let error = decoder
            .decode_eof(&mut compressed_payload)
            .expect_err("decoding should fail");

        let downcasted_error = downcast_framing_error(&error);
        assert!(matches!(
            downcasted_error,
            ChunkedGelfDecoderError::Decompression {
                source: ChunkedGelfDecompressionError::ZlibDecompression { .. }
            }
        ));
    }

    #[tokio::test]
    async fn decode_zlib_payload_with_zlib_decoder() {
        let payload = "foo";
        let compressed_payload = Compression::Zlib.compress(&payload);
        let mut decoder = ChunkedGelfDecoder {
            decompression_config: ChunkedGelfDecompressionConfig::Zlib,
            ..Default::default()
        };

        let frame = decoder
            .decode_eof(&mut compressed_payload.into())
            .expect("decoding should not fail")
            .expect("decoding should return a frame");

        assert_eq!(frame, payload);
    }

    #[tokio::test]
    async fn decode_gzip_payload_with_zlib_decoder() {
        let payload = "foo";
        let compressed_payload = Compression::Gzip.compress(&payload);
        let mut decoder = ChunkedGelfDecoder {
            decompression_config: ChunkedGelfDecompressionConfig::Zlib,
            ..Default::default()
        };

        let error = decoder
            .decode_eof(&mut compressed_payload.into())
            .expect_err("decoding should fail");

        let downcasted_error = downcast_framing_error(&error);
        assert!(matches!(
            downcasted_error,
            ChunkedGelfDecoderError::Decompression {
                source: ChunkedGelfDecompressionError::ZlibDecompression { .. }
            }
        ));
    }

    #[tokio::test]
    async fn decode_uncompressed_payload_with_zlib_decoder() {
        let payload = "foo";
        let mut decoder = ChunkedGelfDecoder {
            decompression_config: ChunkedGelfDecompressionConfig::Zlib,
            ..Default::default()
        };

        let error = decoder
            .decode_eof(&mut payload.into())
            .expect_err("decoding should fail");

        let downcasted_error = downcast_framing_error(&error);
        assert!(matches!(
            downcasted_error,
            ChunkedGelfDecoderError::Decompression {
                source: ChunkedGelfDecompressionError::ZlibDecompression { .. }
            }
        ));
    }

    #[tokio::test]
    async fn decode_gzip_payload_with_gzip_decoder() {
        let payload = "foo";
        let compressed_payload = Compression::Gzip.compress(&payload);
        let mut decoder = ChunkedGelfDecoder {
            decompression_config: ChunkedGelfDecompressionConfig::Gzip,
            ..Default::default()
        };

        let frame = decoder
            .decode_eof(&mut compressed_payload.into())
            .expect("decoding should not fail")
            .expect("decoding should return a frame");

        assert_eq!(frame, payload);
    }

    #[tokio::test]
    async fn decode_zlib_payload_with_gzip_decoder() {
        let payload = "foo";
        let compressed_payload = Compression::Zlib.compress(&payload);
        let mut decoder = ChunkedGelfDecoder {
            decompression_config: ChunkedGelfDecompressionConfig::Gzip,
            ..Default::default()
        };

        let error = decoder
            .decode_eof(&mut compressed_payload.into())
            .expect_err("decoding should fail");

        let downcasted_error = downcast_framing_error(&error);
        assert!(matches!(
            downcasted_error,
            ChunkedGelfDecoderError::Decompression {
                source: ChunkedGelfDecompressionError::GzipDecompression { .. }
            }
        ));
    }

    #[tokio::test]
    async fn decode_uncompressed_payload_with_gzip_decoder() {
        let payload = "foo";
        let mut decoder = ChunkedGelfDecoder {
            decompression_config: ChunkedGelfDecompressionConfig::Gzip,
            ..Default::default()
        };

        let error = decoder
            .decode_eof(&mut payload.into())
            .expect_err("decoding should fail");

        let downcasted_error = downcast_framing_error(&error);
        assert!(matches!(
            downcasted_error,
            ChunkedGelfDecoderError::Decompression {
                source: ChunkedGelfDecompressionError::GzipDecompression { .. }
            }
        ));
    }

    #[tokio::test]
    #[rstest]
    #[case::gzip(Compression::Gzip)]
    #[case::zlib(Compression::Zlib)]
    async fn decode_compressed_payload_with_no_decompression_decoder(
        #[case] compression: Compression,
    ) {
        let payload = "foo";
        let compressed_payload = compression.compress(&payload);
        let mut decoder = ChunkedGelfDecoder {
            decompression_config: ChunkedGelfDecompressionConfig::None,
            ..Default::default()
        };

        let frame = decoder
            .decode_eof(&mut compressed_payload.clone().into())
            .expect("decoding should not fail")
            .expect("decoding should return a frame");

        assert_eq!(frame, compressed_payload);
    }

    #[test]
    fn detect_gzip_compression() {
        let payload = "foo";

        for level in 0..=9 {
            let level = flate2::Compression::new(level);
            let compressed_payload = Compression::Gzip.compress_with_level(&payload, level);
            let actual = ChunkedGelfDecompression::from_magic(&compressed_payload);
            assert_eq!(
                actual,
                ChunkedGelfDecompression::Gzip,
                "Failed for level {}",
                level.level()
            );
        }
    }

    #[test]
    fn detect_zlib_compression() {
        let payload = "foo";

        for level in 0..=9 {
            let level = flate2::Compression::new(level);
            let compressed_payload = Compression::Zlib.compress_with_level(&payload, level);
            let actual = ChunkedGelfDecompression::from_magic(&compressed_payload);
            assert_eq!(
                actual,
                ChunkedGelfDecompression::Zlib,
                "Failed for level {}",
                level.level()
            );
        }
    }

    #[test]
    fn detect_no_compression() {
        let payload = "foo";

        let detected_compression = ChunkedGelfDecompression::from_magic(&payload.into());

        assert_eq!(detected_compression, ChunkedGelfDecompression::None);
    }
}
