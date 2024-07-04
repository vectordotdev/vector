use crate::StreamDecodingError;

use super::{BoxedFramingError, FramingError};
use bytes::{Buf, Bytes, BytesMut};
use derivative::Derivative;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use thiserror::Error;
use tokio;
use tokio::task::JoinHandle;
use tokio_util::codec::Decoder;
use tracing::{info, warn};
use vector_config::configurable_component;

const GELF_MAGIC: [u8; 2] = [0x1e, 0x0f];
const MAX_TOTAL_CHUNKS: u8 = 128;
const DEFAULT_CHUNKS: [Bytes; MAX_TOTAL_CHUNKS as usize] =
    [const { Bytes::new() }; MAX_TOTAL_CHUNKS as usize];
const DEFAULT_TIMEOUT_MILLIS: u64 = 5000;

/// Config used to build a `ChunkedGelfDecoderConfig`.
#[configurable_component]
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ChunkedGelfDecoderConfig {
    /// Options for the chunked gelf decoder.
    #[serde(default, skip_serializing_if = "vector_core::serde::is_default")]
    pub chunked_gelf: ChunkedGelfDecoderOptions,
}

impl ChunkedGelfDecoderConfig {
    /// Creates a new `BytesDecoderConfig`.
    pub fn new() -> Self {
        Default::default()
    }

    /// Build the `ByteDecoder` from this configuration.
    pub fn build(&self) -> ChunkedGelfDecoder {
        ChunkedGelfDecoder::new(self.chunked_gelf.timeout_millis)
    }
}

const fn default_timeout_millis() -> u64 {
    DEFAULT_TIMEOUT_MILLIS
}

/// Options for building a `ChunkedGelfDecoder`.
#[configurable_component]
#[derive(Clone, Debug, Derivative, PartialEq, Eq)]
pub struct ChunkedGelfDecoderOptions {
    /// The timeout in milliseconds for a message to be fully received.
    #[serde(
        default = "default_timeout_millis",
        skip_serializing_if = "vector_core::serde::is_default"
    )]
    pub timeout_millis: u64,
}

impl Default for ChunkedGelfDecoderOptions {
    fn default() -> Self {
        Self {
            timeout_millis: default_timeout_millis(),
        }
    }
}

#[derive(Debug)]
pub struct MessageState {
    total_chunks: u8,
    chunks: [Bytes; MAX_TOTAL_CHUNKS as usize],
    chunks_bitmap: u128,
    timeout_task: JoinHandle<()>,
}

impl MessageState {
    pub fn new(total_chunks: u8, timeout_task: JoinHandle<()>) -> Self {
        Self {
            total_chunks,
            chunks: DEFAULT_CHUNKS,
            chunks_bitmap: 0,
            timeout_task,
        }
    }

    pub fn is_chunk_present(&self, sequence_number: u8) -> bool {
        let chunk_bitmap_id = 1 << sequence_number;
        self.chunks_bitmap & chunk_bitmap_id != 0
    }

    pub fn add_chunk(&mut self, sequence_number: u8, chunk: Bytes) {
        let chunk_bitmap_id = 1 << sequence_number;
        self.chunks[sequence_number as usize] = chunk;
        self.chunks_bitmap |= chunk_bitmap_id;
    }

    pub fn is_complete(&self) -> bool {
        self.chunks_bitmap.count_ones() == self.total_chunks as u32
    }

    pub fn retrieve_message(&mut self) -> Option<Bytes> {
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

#[derive(Debug, Error)]
pub enum ChunkedGelfDecoderError {
    #[error("Poisoned lock")]
    PoisonedLock,
}

impl From<ChunkedGelfDecoderError> for BoxedFramingError {
    fn from(error: ChunkedGelfDecoderError) -> Self {
        Box::new(error)
    }
}
impl StreamDecodingError for ChunkedGelfDecoderError {
    fn can_continue(&self) -> bool {
        match self {
            ChunkedGelfDecoderError::PoisonedLock => false,
        }
    }
}

impl FramingError for ChunkedGelfDecoderError {}

/// A decoder for handling GELF messages that are chunked.
#[derive(Debug)]
pub struct ChunkedGelfDecoder {
    /// TODO
    state: Arc<Mutex<HashMap<u64, MessageState>>>,
    timeout: Duration,
}

impl Clone for ChunkedGelfDecoder {
    fn clone(&self) -> Self {
        Self {
            state: Arc::new(Mutex::new(HashMap::new())),
            timeout: self.timeout,
        }
    }
}

impl ChunkedGelfDecoder {
    /// Creates a new `ChunkedGelfDecoder`.
    pub fn new(timeout_millis: u64) -> Self {
        Self {
            state: Arc::new(Mutex::new(HashMap::new())),
            timeout: Duration::from_millis(timeout_millis),
        }
    }

    /// TODO: document this
    pub fn decode_chunk(
        &mut self,
        src: &mut bytes::BytesMut,
    ) -> Result<Option<Bytes>, ChunkedGelfDecoderError> {
        // We need 10 bits to read the message id, sequence number and total chunks
        if src.remaining() < 10 {
            let src_display = format!("{src:?}");
            warn!(message = "Received malformed chunk headers (message ID, sequence number and total chunks) with less than 10 bytes. Ignoring it.",
                src = src_display,
                remaining = src.remaining(),
                internal_log_rate_limit = true);
            src.clear();
            return Ok(None);
        }
        let message_id = src.get_u64();
        let sequence_number = src.get_u8();
        let total_chunks = src.get_u8();

        if total_chunks == 0 || total_chunks > MAX_TOTAL_CHUNKS {
            warn!(
                message = "Received a chunk with an invalid total chunks value. Ignoring it.",
                message_id = message_id,
                sequence_number = sequence_number,
                total_chunks = total_chunks,
                internal_log_rate_limit = true
            );
            src.clear();
            return Ok(None);
        }

        if sequence_number >= total_chunks {
            warn!(
                message = "Received a chunk with a sequence number greater than total chunks. Ignoring it.",
                message_id = message_id,
                sequence_number = sequence_number,
                total_chunks = total_chunks,
                internal_log_rate_limit = true
            );
            src.clear();
            return Ok(None);
        }

        // TODO: handle this unwrap
        let Ok(mut state_lock) = self.state.lock() else {
            return Err(ChunkedGelfDecoderError::PoisonedLock);
        };

        let message_state = state_lock.entry(message_id).or_insert_with(|| {
            // TODO: we need tokio due to the sleep function. We need to spawn a task that will clear the message state after a certain time
            // otherwise we will have a memory leak
            let state = Arc::clone(&self.state);
            let timeout = self.timeout.clone();
            let timeout_handle = tokio::spawn(async move {
                tokio::time::sleep(timeout).await;
                let mut state_lock = state.lock().unwrap();
                if let Some(_) = state_lock.remove(&message_id) {
                    let message = format!(
                        "Message was not fully received within the timeout window. Discarding it."
                    );
                    // TODO: log the variables in the message or use structured logging?
                    warn!(
                        message = message,
                        message_id = message_id,
                        timeout = timeout.as_millis(),
                        internal_log_rate_limit = true
                    );
                }
            });
            MessageState::new(total_chunks, timeout_handle)
        });

        if message_state.total_chunks != total_chunks {
            warn!(message_id = "Received a chunk with a different total_chunks than the original. Ignoring it.",
                original_total_chunks = message_state.total_chunks,
                received_total_chunks = total_chunks,
                internal_log_rate_limit = true,
                message_id = message_id);
            src.clear();
            return Ok(None);
        }

        if message_state.is_chunk_present(sequence_number) {
            info!(
                message = "Received a duplicate chunk. Ignoring it.",
                sequence_number = sequence_number,
                message_id = message_id,
                internal_log_rate_limit = true
            );
            src.clear();
            return Ok(None);
        }

        let chunk = src.split().freeze();
        message_state.add_chunk(sequence_number, chunk);

        if let Some(message) = message_state.retrieve_message() {
            state_lock.remove(&message_id);
            Ok(Some(message))
        } else {
            Ok(None)
        }
    }
}

impl Default for ChunkedGelfDecoder {
    fn default() -> Self {
        Self::new(DEFAULT_TIMEOUT_MILLIS)
    }
}

impl Decoder for ChunkedGelfDecoder {
    type Item = Bytes;

    type Error = BoxedFramingError;

    fn decode(&mut self, src: &mut bytes::BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        if src.is_empty() {
            return Ok(None);
        }

        let magic = src.get(0..2);
        if magic.is_some_and(|magic| magic == GELF_MAGIC) {
            src.advance(2);
            Ok(self.decode_chunk(src)?)
        } else {
            // The gelf message is not chunked
            let frame = src.split();
            return Ok(Some(frame.freeze()));
        }
    }

    // TODO: implement decode_eof
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::{BufMut, BytesMut};
    use rstest::{fixture, rstest};

    fn create_chunk(
        message_id: u64,
        sequence_number: u8,
        total_chunks: u8,
        payload: &str,
    ) -> Bytes {
        let mut chunk = BytesMut::new();
        chunk.put_slice(&GELF_MAGIC);
        chunk.put_u64(message_id);
        chunk.put_u8(sequence_number);
        chunk.put_u8(total_chunks);
        chunk.extend_from_slice(payload.as_bytes());
        chunk.freeze()
    }

    #[fixture]
    fn unchunked_message() -> (Bytes, String) {
        let payload = "foo";
        (Bytes::from(payload), payload.to_string())
    }

    // TODO: add a malformed chunk message

    #[fixture]
    fn two_chunks_message() -> ([Bytes; 2], String) {
        let message_id = 1u64;
        let total_chunks = 2u8;

        let first_sequence_number = 0u8;
        let first_payload = "foo";
        let first_chunk = create_chunk(
            message_id,
            first_sequence_number,
            total_chunks,
            first_payload,
        );

        let second_sequence_number = 1u8;
        let second_payload = "bar";
        let second_chunk = create_chunk(
            message_id,
            second_sequence_number,
            total_chunks,
            second_payload,
        );

        (
            [first_chunk, second_chunk],
            format!("{first_payload}{second_payload}"),
        )
    }

    #[fixture]
    fn three_chunks_message() -> ([Bytes; 3], String) {
        let message_id = 2u64;
        let total_chunks = 3u8;

        let first_sequence_number = 0u8;
        let first_payload = "foo";
        let first_chunk = create_chunk(
            message_id,
            first_sequence_number,
            total_chunks,
            first_payload,
        );

        let second_sequence_number = 1u8;
        let second_payload = "bar";
        let second_chunk = create_chunk(
            message_id,
            second_sequence_number,
            total_chunks,
            second_payload,
        );

        let third_sequence_number = 2u8;
        let third_payload = "baz";
        let third_chunk = create_chunk(
            message_id,
            third_sequence_number,
            total_chunks,
            third_payload,
        );

        (
            [first_chunk, second_chunk, third_chunk],
            format!("{first_payload}{second_payload}{third_payload}"),
        )
    }

    #[rstest]
    #[tokio::test]
    async fn decode_chunked(two_chunks_message: ([Bytes; 2], String)) {
        let mut src = BytesMut::new();
        let (chunks, expected_message) = two_chunks_message;
        let mut decoder = ChunkedGelfDecoder::default();

        src.extend_from_slice(&chunks[0]);
        let frame = decoder.decode(&mut src).unwrap();
        assert!(frame.is_none());

        src.extend_from_slice(&chunks[1]);
        let frame = decoder.decode(&mut src).unwrap();

        assert_eq!(frame, Some(Bytes::from(expected_message)));
    }

    #[rstest]
    #[tokio::test]
    async fn decode_unchunked(unchunked_message: (Bytes, String)) {
        let mut src = BytesMut::new();
        let (message, expected_message) = unchunked_message;
        let mut decoder = ChunkedGelfDecoder::default();

        src.extend_from_slice(&message);
        let frame = decoder.decode(&mut src).unwrap();
        assert_eq!(frame, Some(Bytes::from(expected_message)));
    }

    #[rstest]
    #[tokio::test]
    async fn decode_unordered_chunks(two_chunks_message: ([Bytes; 2], String)) {
        let mut src = BytesMut::new();
        let (chunks, expected_message) = two_chunks_message;
        let mut decoder = ChunkedGelfDecoder::default();

        src.extend_from_slice(&chunks[1]);
        let frame = decoder.decode(&mut src).unwrap();
        assert!(frame.is_none());

        src.extend_from_slice(&chunks[0]);
        let frame = decoder.decode(&mut src).unwrap();

        assert_eq!(frame, Some(Bytes::from(expected_message)));
    }

    #[rstest]
    #[tokio::test]
    async fn decode_unordered_messages(
        two_chunks_message: ([Bytes; 2], String),
        three_chunks_message: ([Bytes; 3], String),
    ) {
        let mut src = BytesMut::new();
        let (two_chunks, two_chunks_expected) = two_chunks_message;
        let (three_chunks, three_chunks_expected) = three_chunks_message;
        let mut decoder = ChunkedGelfDecoder::default();

        src.extend_from_slice(&three_chunks[2]);
        let frame = decoder.decode(&mut src).unwrap();
        assert!(frame.is_none());

        src.extend_from_slice(&two_chunks[0]);
        let frame = decoder.decode(&mut src).unwrap();
        assert!(frame.is_none());

        src.extend_from_slice(&three_chunks[0]);
        let frame = decoder.decode(&mut src).unwrap();
        assert!(frame.is_none());

        src.extend_from_slice(&two_chunks[1]);
        let frame = decoder.decode(&mut src).unwrap();
        assert_eq!(frame, Some(Bytes::from(two_chunks_expected)));

        src.extend_from_slice(&three_chunks[1]);
        let frame = decoder.decode(&mut src).unwrap();
        assert_eq!(frame, Some(Bytes::from(three_chunks_expected)));
    }

    #[rstest]
    #[tokio::test]
    async fn decode_mixed_chunked_and_unchunked_messages(
        unchunked_message: (Bytes, String),
        two_chunks_message: ([Bytes; 2], String),
    ) {
        let mut src = BytesMut::new();
        let (unchunked_message, expected_unchunked_message) = unchunked_message;
        let (chunks, expected_chunked_message) = two_chunks_message;
        let mut decoder = ChunkedGelfDecoder::default();

        src.extend_from_slice(&chunks[1]);
        let frame = decoder.decode(&mut src).unwrap();
        assert!(frame.is_none());

        src.extend_from_slice(&unchunked_message);
        let frame = decoder.decode(&mut src).unwrap();
        assert_eq!(frame, Some(Bytes::from(expected_unchunked_message)));

        src.extend_from_slice(&chunks[0]);
        let frame = decoder.decode(&mut src).unwrap();
        assert_eq!(frame, Some(Bytes::from(expected_chunked_message)));
    }

    #[rstest]
    #[tokio::test(start_paused = true)]
    async fn decode_timeout(two_chunks_message: ([Bytes; 2], String)) {
        let timeout = 300;
        let mut src = BytesMut::new();
        let (chunks, _) = two_chunks_message;
        let mut decoder = ChunkedGelfDecoder::new(timeout);

        src.extend_from_slice(&chunks[0]);
        let frame = decoder.decode(&mut src).unwrap();
        assert!(frame.is_none());
        assert!(!decoder.state.lock().unwrap().is_empty());

        // The message state should be cleared after a certain time
        tokio::time::sleep(Duration::from_millis(timeout + 1)).await;
        assert!(decoder.state.lock().unwrap().is_empty());

        src.extend_from_slice(&chunks[1]);
        let frame = decoder.decode(&mut src).unwrap();
        assert!(frame.is_none());

        tokio::time::sleep(Duration::from_millis(timeout + 1)).await;
        assert!(decoder.state.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn decode_empty_input() {
        let mut src = BytesMut::new();
        let mut decoder = ChunkedGelfDecoder::default();

        let frame = decoder.decode(&mut src).unwrap();
        assert!(frame.is_none());
    }
}
